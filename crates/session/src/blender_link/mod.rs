//! The live Blender link, one of the modder tools: a button beside pause
//! starts a listener that Blender's OpenZT2 add-on uses to open the selected
//! object's model and send edited models back while the game runs.

mod link_server;
mod mod_archive;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use bevy::{
    asset::{embedded_asset, load_embedded_asset, AssetPath},
    prelude::*,
    ui::Val2,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    asset_source::AssetArchives,
    material::material_asset_types::MaterialAsset,
    model_source::{
        editable_gltf_model_export::{encode_editable_gltf_model, place_gltf_scene},
        gltf_model_source::validate_gltf_model_replacement,
    },
    scene_prefab::ScenePrefabAsset,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::{
    scene_prefab::ScenePrefabAssetDependencyKind,
    ui_document::action::simulation_time::UiSimulationAction,
};
use openzt2_interface_state::{
    information::entity_selection_types::SelectedEntity,
    settings::modder_tools_settings_types::ModderToolsSettings,
    ui::{
        authored_ui_action_projection_components::UiSimulationActions,
        authored_ui_interaction_enabled_state::UiInteractionEnabled,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        pointer_capture_types::UiPointerInteraction,
        UiSet,
    },
};
use openzt2_world_state::{
    application_schedule::GameSet,
    world_spawn::{
        prefab_model_replacement::PrefabModelReplaced,
        prefab_presentation_types::PrefabPresentation,
        prefab_source_asset_handle::PrefabSourceAssetHandle,
    },
};
use serde_json::{json, Value};

use crate::persistence::persistence_filesystem_paths::PersistenceFilesystemPaths;

use self::link_server::{LinkRequest, LinkServer, BLENDER_LINK_PORT};

/// How long a Load waits for the object's materials before exporting without
/// the textures that have not loaded.
const MATERIAL_LOAD_TIMEOUT: Duration = Duration::from_secs(15);

pub struct BlenderLinkPlugin;

impl Plugin for BlenderLinkPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "button.png");
        app.init_resource::<BlenderLink>()
            .add_systems(
                Update,
                toggle_modder_tools
                    .in_set(UiSet::Interaction)
                    .run_if(in_state(GamePhase::MainMenu)),
            )
            .add_systems(
                Update,
                (attach_blender_button, present_blender_button)
                    .chain()
                    .in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                (toggle_blender_link, stop_blender_link_without_modder_tools)
                    .chain()
                    .in_set(GameSet::Input)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(Update, answer_blender_link_requests.in_set(GameSet::Ui))
            .add_systems(OnExit(GamePhase::InGame), stop_blender_link);
    }
}

#[derive(Resource, Default)]
struct BlenderLink {
    server: Option<LinkServer>,
    pending_loads: Vec<PendingLoad>,
}

#[derive(Component)]
pub(crate) struct ModderToolsCheckbox;

pub(crate) fn modder_tools_checkbox_label(enabled: bool) -> String {
    format!("[{}] Modder tools", if enabled { 'x' } else { ' ' })
}

fn toggle_modder_tools(
    mut settings: ResMut<ModderToolsSettings>,
    mut checkboxes: Query<(Ref<UiPointerInteraction>, &mut Text), With<ModderToolsCheckbox>>,
) {
    for (interaction, mut label) in &mut checkboxes {
        if interaction.is_changed() && *interaction == UiPointerInteraction::Pressed {
            settings.enabled = !settings.enabled;
        }
        let expected = modder_tools_checkbox_label(settings.enabled);
        if label.0 != expected {
            label.0 = expected;
        }
    }
}

#[derive(Component)]
struct BlenderButton {
    pause: Entity,
}

fn attach_blender_button(
    mut commands: Commands,
    assets: Res<AssetServer>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    controls: Query<
        (
            Entity,
            &UiSimulationActions,
            &UiDocumentOwner,
            &ChildOf,
            &ZIndex,
        ),
        Added<UiSimulationActions>,
    >,
) {
    for (pause, actions, owner, parent, z) in &controls {
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if !actions.authored_action_records(document).any(|record| {
            matches!(
                record.action,
                UiSimulationAction::ToggleSimulationPaused
                    | UiSimulationAction::SetSimulationPaused { .. }
            )
        }) {
            continue;
        }
        commands.spawn((
            Name::new("Blender link"),
            BlenderButton { pause },
            ChildOf(parent.parent()),
            ZIndex(z.0 + 1),
            bevy::ui_widgets::Button,
            UiInteractionEnabled(false),
            Visibility::Hidden,
            ImageNode::new(load_embedded_asset!(&*assets, "button.png")),
            UiTransform {
                // Below the pause button, clear of the fame stars beside it.
                translation: Val2::percent(-10.0, 95.0),
                scale: Vec2::splat(0.6),
                ..default()
            },
        ));
    }
}

fn present_blender_button(
    mut commands: Commands,
    settings: Res<ModderToolsSettings>,
    link: Res<BlenderLink>,
    pause_controls: Query<(&Node, &Visibility), Without<BlenderButton>>,
    mut buttons: Query<(
        Entity,
        &BlenderButton,
        &UiPointerInteraction,
        &mut Node,
        &mut Visibility,
        &mut ImageNode,
        &mut UiInteractionEnabled,
    )>,
) {
    for (entity, button, interaction, mut node, mut visibility, mut image, mut interactive) in
        &mut buttons
    {
        let Ok((pause_layout, pause_visibility)) = pause_controls.get(button.pause) else {
            commands.entity(entity).despawn();
            continue;
        };
        if *node != *pause_layout {
            *node = pause_layout.clone();
        }
        visibility.set_if_neq(if settings.enabled {
            *pause_visibility
        } else {
            Visibility::Hidden
        });
        interactive.set_if_neq(UiInteractionEnabled(settings.enabled));
        let x = match (link.server.is_some(), interaction) {
            (_, UiPointerInteraction::Pressed) => 192.0,
            (_, UiPointerInteraction::Hovered) => 128.0,
            (true, UiPointerInteraction::None) => 64.0,
            (false, UiPointerInteraction::None) => 0.0,
        };
        let rect = Some(Rect::new(x, 0.0, x + 64.0, 64.0));
        if image.rect != rect {
            image.rect = rect;
        }
    }
}

fn toggle_blender_link(
    mut link: ResMut<BlenderLink>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
    buttons: Query<
        (&UiPointerInteraction, &UiInteractionEnabled),
        (With<BlenderButton>, Changed<UiPointerInteraction>),
    >,
) {
    if !buttons
        .iter()
        .any(|(interaction, enabled)| enabled.0 && *interaction == UiPointerInteraction::Pressed)
    {
        return;
    }
    if link.server.is_some() {
        stop(&mut link, &archives, &mut replaced);
        return;
    }
    match LinkServer::start() {
        Ok(server) => {
            info!("Blender link listening on 127.0.0.1:{BLENDER_LINK_PORT}");
            link.server = Some(server);
        }
        Err(error) => error!("Blender link could not start on port {BLENDER_LINK_PORT}: {error}"),
    }
}

fn stop_blender_link_without_modder_tools(
    settings: Res<ModderToolsSettings>,
    mut link: ResMut<BlenderLink>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
) {
    if !settings.enabled && link.server.is_some() {
        stop(&mut link, &archives, &mut replaced);
    }
}

fn stop_blender_link(
    mut link: ResMut<BlenderLink>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
) {
    stop(&mut link, &archives, &mut replaced);
}

/// Stops the link and returns every model it replaced to the original.
fn stop(
    link: &mut BlenderLink,
    archives: &AssetArchives,
    replaced: &mut MessageWriter<PrefabModelReplaced>,
) {
    if link.server.take().is_some() {
        info!("Blender link stopped");
    }
    link.pending_loads.clear();
    match archives.restore_live_model_replacements() {
        Ok(restored) => announce(replaced, restored),
        Err(error) => error!("models replaced from Blender could not be restored: {error}"),
    }
}

/// Tells objects using these models to rebuild once the models reload.
fn announce(
    replaced: &mut MessageWriter<PrefabModelReplaced>,
    models: impl IntoIterator<Item = impl AsRef<Path>>,
) {
    replaced.write_batch(models.into_iter().map(|model| PrefabModelReplaced {
        model_path: model.as_ref().to_string_lossy().into_owned(),
    }));
}


/// One prefab presentation within the selected object.
struct ObjectPart {
    model: String,
    prefab: Handle<ScenePrefabAsset>,
    /// Placement relative to the object.
    transform: Transform,
    visible: bool,
}

/// A Load waiting for its materials to load through the game's own loaders,
/// so their textures can be exported with the models.
struct PendingLoad {
    request: LinkRequest,
    object: String,
    parts: Vec<ObjectPart>,
    materials: Vec<(String, Handle<MaterialAsset>)>,
    started: Instant,
}

#[derive(bevy::ecs::system::SystemParam)]
struct LinkWorld<'w, 's> {
    selected: Res<'w, SelectedEntity>,
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
    presentations: Query<
        'w,
        's,
        (
            AnyOf<(&'static PrefabSourceAssetHandle, &'static PrefabPresentation)>,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    asset_server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    materials: Res<'w, Assets<MaterialAsset>>,
    archives: Res<'w, AssetArchives>,
    paths: Option<Res<'w, PersistenceFilesystemPaths>>,
}

fn answer_blender_link_requests(
    mut link: ResMut<BlenderLink>,
    world: LinkWorld,
    mut replaced: MessageWriter<PrefabModelReplaced>,
) {
    let mut new_loads = Vec::new();
    let mut replaced_models = Vec::new();
    if let Some(server) = &link.server {
        while let Some(request) = server.try_next_request() {
            if request.method == "load_object" {
                match begin_load(&world, &mut replaced_models) {
                    Ok((object, parts, materials)) => new_loads.push(PendingLoad {
                        request,
                        object,
                        parts,
                        materials,
                        started: Instant::now(),
                    }),
                    Err(message) => request.answer(Err(message)),
                }
            } else {
                let result = answer(&request, &world, &mut replaced_models);
                request.answer(result);
            }
        }
    }
    announce(&mut replaced, replaced_models);
    link.pending_loads.extend(new_loads);
    let (ready, waiting) = std::mem::take(&mut link.pending_loads)
        .into_iter()
        .partition::<Vec<_>, _>(|load| {
            load.started.elapsed() > MATERIAL_LOAD_TIMEOUT
                || load.materials.iter().all(|(_, material)| {
                    world.materials.contains(material)
                        || world.asset_server.load_state(material).is_failed()
                })
        });
    link.pending_loads = waiting;
    for load in ready {
        let result = finish_load(&load, &world);
        load.request.answer(result);
    }
}

fn answer(
    request: &LinkRequest,
    world: &LinkWorld,
    replaced_models: &mut Vec<String>,
) -> Result<Value, String> {
    match request.method.as_str() {
        "status" => Ok(json!({
            "game": "OpenZT2",
            "version": env!("CARGO_PKG_VERSION"),
            "selected_object": selected_object(world).map(|(object, _)| object),
        })),
        "replace_model" => {
            let model = request
                .params
                .get("model")
                .and_then(Value::as_str)
                .ok_or("replace_model needs a model path")?;
            validate_gltf_model_replacement(&request.model)
                .map_err(|error| format!("{error:#}"))?;
            world
                .archives
                .replace_model_live(Path::new(model), request.model.clone())
                .map_err(|error| error.to_string())?;
            replaced_models.push(model.to_owned());
            Ok(json!({ "model": model }))
        }
        "save_as_mod" => {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .ok_or("save_as_mod needs a mod name")?;
            let replacements = world.archives.live_model_replacements();
            let path = mod_archive::write_mod_archive(
                world.archives.installation_root(),
                name,
                &replacements,
            )?;
            let models = replacements
                .iter()
                .map(|(model, _)| model.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            Ok(json!({ "path": path, "models": models }))
        }
        "restore_models" => {
            let restored = world
                .archives
                .restore_live_model_replacements()
                .map_err(|error| error.to_string())?;
            replaced_models.extend(
                restored
                    .iter()
                    .map(|model| model.to_string_lossy().into_owned()),
            );
            Ok(json!({}))
        }
        method => Err(format!("unknown method {method}")),
    }
}

/// The selected object's name and its models. An object is a small scene: a
/// building has its own model, built onto the object, and attached parts
/// such as components and states.
fn selected_object(world: &LinkWorld) -> Option<(String, Vec<ObjectPart>)> {
    let selected = world.selected.0?;
    let object_transform = world.transforms.get(selected).ok()?;
    let mut parts = Vec::<ObjectPart>::new();
    for entity in std::iter::once(selected).chain(world.children.iter_descendants(selected)) {
        let Ok(((source, presentation), transform, visibility)) = world.presentations.get(entity)
        else {
            continue;
        };
        let Some(prefab) = source
            .map(|source| &source.0)
            .or(presentation.map(|presentation| &presentation.0))
        else {
            continue;
        };
        let Some(model) = prefab
            .path()
            .map(|path| path.path().to_string_lossy().into_owned())
        else {
            continue;
        };
        // A model used twice is edited once.
        if parts.iter().any(|part| part.model == model) {
            continue;
        }
        parts.push(ObjectPart {
            model,
            prefab: prefab.clone(),
            transform: transform.reparented_to(object_transform),
            visible: visibility.get(),
        });
    }
    let first = parts.first()?;
    let object = world
        .names
        .get(selected)
        .ok()
        .map(|name| name.as_str().trim())
        .filter(|name| !name.is_empty())
        .map_or_else(
            || {
                Path::new(&first.model)
                    .file_stem()
                    .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
            },
            str::to_owned,
        );
    Some((object, parts))
}

type BegunLoad = (String, Vec<ObjectPart>, Vec<(String, Handle<MaterialAsset>)>);

fn begin_load(world: &LinkWorld, replaced_models: &mut Vec<String>) -> Result<BegunLoad, String> {
    let (object, parts) = selected_object(world).ok_or("no object is selected in the game")?;
    // Loading starts again from the original, discarding synced edits.
    for part in &parts {
        if world
            .archives
            .restore_live_model_replacement(Path::new(&part.model))
            .map_err(|error| error.to_string())?
        {
            replaced_models.push(part.model.clone());
        }
    }
    let mut materials = Vec::new();
    for part in &parts {
        let Some(prefab) = world.prefabs.get(&part.prefab) else {
            continue;
        };
        for dependency in &prefab.canonical_scene_prefab_document().dependencies {
            if dependency.asset_kind == ScenePrefabAssetDependencyKind::Material
                && !materials
                    .iter()
                    .any(|(path, _)| *path == dependency.asset_path)
            {
                let handle = world
                    .asset_server
                    .load::<MaterialAsset>(material_asset_path(&dependency.asset_path));
                materials.push((dependency.asset_path.clone(), handle));
            }
        }
    }
    Ok((object, parts, materials))
}

/// Writes each part as its own GLB, placed where it sits in the object, for
/// Blender's importer to read.
fn finish_load(load: &PendingLoad, world: &LinkWorld) -> Result<Value, String> {
    let directory = world
        .paths
        .as_ref()
        .ok_or("the game has no folder to write models to")?
        .blender_model_export_directory_path();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    for part in &load.parts {
        let glb = encode_editable_gltf_model(&world.archives, &part.model, |_, material_path| {
            let (_, handle) = load
                .materials
                .iter()
                .find(|(path, _)| path == material_path)?;
            let texture = world.materials.get(handle)?.first_bound_texture_asset()?;
            Some(texture.path()?.path().to_string_lossy().into_owned())
        })
        .and_then(|glb| {
            place_gltf_scene(
                &glb,
                part.transform.translation.to_array(),
                part.transform.rotation.to_array(),
                part.transform.scale.to_array(),
            )
        })
        .map_err(|error| format!("{} could not be exported: {error:#}", part.model))?;
        let path = directory.join(format!(
            "{}.glb",
            Path::new(&part.model)
                .with_extension("")
                .to_string_lossy()
                .replace(['/', '\\'], "_")
        ));
        std::fs::write(&path, glb).map_err(|error| error.to_string())?;
        parts.push(json!({ "model": part.model, "path": path, "visible": part.visible }));
    }
    Ok(json!({ "object": load.object, "parts": parts }))
}

/// Generated native material paths use '#' as Bevy's label delimiter; a
/// `.bfmat` file name may contain '#' itself.
fn material_asset_path(material_path: &str) -> AssetPath<'static> {
    if material_path.to_ascii_lowercase().ends_with(".bfmat") {
        AssetPath::from_path_buf(material_path.into())
    } else {
        AssetPath::from(material_path.to_owned())
    }
}

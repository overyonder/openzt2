//! The live Blender link, one of the modder tools: a button beside pause
//! starts a listener that Blender's OpenZT2 add-on uses to open the selected
//! object's model and send edited models back while the game runs.

mod link_server;
mod mod_archive;

use bevy::{
    picking::{events::PointerPress, hover::DirectlyHovered, pointer::PointerButton},
    ui::Pressed,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    asset::{embedded_asset, load_embedded_asset, AssetLoadFailedEvent, AssetPath},
    prelude::*,
    tasks::{block_on, poll_once, AsyncComputeTaskPool, IoTaskPool, Task},
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
    information::entity_selection_types::WorldUiSelection,
    settings::modder_tools_settings_types::ModderToolsSettings,
    ui::{
        authored_ui_action_projection_components::UiSimulationActions,
        authored_ui_interaction_enabled_state::UiInteractionEnabled,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
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
        app.add_systems(
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
        .add_systems(
            Update,
            (
                receive_blender_link_requests,
                export_blender_objects_with_ready_materials,
                answer_completed_blender_exports,
            )
                .chain()
                .in_set(GameSet::Ui),
        )
        .add_systems(OnExit(GamePhase::InGame), stop_blender_link);
    }
}

#[derive(Component)]
struct BlenderLink {
    server: Option<LinkServer>,
}

#[derive(Component)]
pub(crate) struct ModderToolsCheckbox;

pub(crate) fn modder_tools_checkbox_label(enabled: bool) -> String {
    format!("[{}] Modder tools", if enabled { 'x' } else { ' ' })
}

fn toggle_modder_tools(
    mut settings: ResMut<ModderToolsSettings>,
    mut presses: MessageReader<PointerPress>,
    mut checkboxes: Query<&mut Text, With<ModderToolsCheckbox>>,
) {
    for press in presses.read() {
        if press.pointer.id.is_mouse()
            && press.button == PointerButton::Primary
            && checkboxes.contains(press.entity)
        {
            settings.enabled = !settings.enabled;
        }
    }
    let expected = modder_tools_checkbox_label(settings.enabled);
    for mut label in &mut checkboxes {
        if label.0 != expected {
            label.0.clone_from(&expected);
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
            DirectlyHovered::default(),
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
    link: Query<&BlenderLink>,
    pause_controls: Query<(&Node, &Visibility), Without<BlenderButton>>,
    mut buttons: Query<(
        Entity,
        &BlenderButton,
        &DirectlyHovered,
        Has<Pressed>,
        &mut Node,
        &mut Visibility,
        &mut ImageNode,
        &mut UiInteractionEnabled,
    )>,
) {
    for (entity, button, hovered, pressed, mut node, mut visibility, mut image, mut interactive) in
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
        let x = match (!link.is_empty(), hovered.get(), pressed) {
            (_, _, true) => 192.0,
            (_, true, false) => 128.0,
            (true, false, false) => 64.0,
            (false, false, false) => 0.0,
        };
        let rect = Some(Rect::new(x, 0.0, x + 64.0, 64.0));
        if image.rect != rect {
            image.rect = rect;
        }
    }
}

fn toggle_blender_link(
    mut commands: Commands,
    mut links: Query<(Entity, &mut BlenderLink)>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
    mut presses: MessageReader<PointerPress>,
    buttons: Query<&UiInteractionEnabled, With<BlenderButton>>,
    mut requests: Query<(&ChildOf, &mut BlenderExportRequest)>,
) {
    let pressed = presses.read().any(|press| {
        press.pointer.id.is_mouse()
            && press.button == PointerButton::Primary
            && buttons.get(press.entity).is_ok_and(|enabled| enabled.0)
    });
    presses.clear();
    if !pressed {
        return;
    }
    if let Ok((owner, mut link)) = links.single_mut() {
        stop(
            &mut commands,
            owner,
            &mut link,
            &archives,
            &mut replaced,
            &mut requests,
        );
        return;
    }
    match LinkServer::start() {
        Ok(server) => {
            info!("Blender link listening on 127.0.0.1:{BLENDER_LINK_PORT}");
            commands.spawn((
                Name::new("Blender link"),
                BlenderLink {
                    server: Some(server),
                },
            ));
        }
        Err(error) => error!("Blender link could not start on port {BLENDER_LINK_PORT}: {error}"),
    }
}

fn stop_blender_link_without_modder_tools(
    mut commands: Commands,
    settings: Res<ModderToolsSettings>,
    mut links: Query<(Entity, &mut BlenderLink)>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
    mut requests: Query<(&ChildOf, &mut BlenderExportRequest)>,
) {
    if !settings.enabled {
        if let Ok((owner, mut link)) = links.single_mut() {
            stop(
                &mut commands,
                owner,
                &mut link,
                &archives,
                &mut replaced,
                &mut requests,
            );
        }
    }
}

fn stop_blender_link(
    mut commands: Commands,
    mut links: Query<(Entity, &mut BlenderLink)>,
    archives: Res<AssetArchives>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
    mut requests: Query<(&ChildOf, &mut BlenderExportRequest)>,
) {
    if let Ok((owner, mut link)) = links.single_mut() {
        stop(
            &mut commands,
            owner,
            &mut link,
            &archives,
            &mut replaced,
            &mut requests,
        );
    }
}

/// Stops the link and returns every model it replaced to the original.
fn stop(
    commands: &mut Commands,
    owner: Entity,
    link: &mut BlenderLink,
    archives: &AssetArchives,
    replaced: &mut MessageWriter<PrefabModelReplaced>,
    requests: &mut Query<(&ChildOf, &mut BlenderExportRequest)>,
) {
    if let Some(server) = link.server.take() {
        for (parent, mut request) in requests {
            if parent.parent() == owner {
                if let Some(request) = request.0.take() {
                    request.answer(Err("Blender link stopped".to_owned()));
                }
            }
        }
        commands.entity(owner).despawn();
        drop(server);
        info!("Blender link stopped");
    }
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
#[derive(Component)]
struct BlenderObjectMaterialLoad {
    object: String,
    parts: Vec<ObjectPart>,
    materials: Vec<(String, Handle<MaterialAsset>)>,
    deadline: Instant,
}

#[derive(Component)]
struct BlenderExportRequest(Option<LinkRequest>);

#[derive(Component)]
struct BlenderExportTask(Task<Result<Value, String>>);

#[derive(bevy::ecs::system::SystemParam)]
struct LinkWorld<'w, 's> {
    selected: WorldUiSelection<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
    presentations: Query<
        'w,
        's,
        (
            AnyOf<(
                &'static PrefabSourceAssetHandle,
                &'static PrefabPresentation,
            )>,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    asset_server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    archives: Res<'w, AssetArchives>,
}

fn answer(
    request: &mut LinkRequest,
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
                .replace_model_live(Path::new(model), std::mem::take(&mut request.model))
                .map_err(|error| error.to_string())?;
            replaced_models.push(model.to_owned());
            Ok(json!({ "model": model }))
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

fn receive_blender_link_requests(
    mut commands: Commands,
    links: Query<(Entity, &BlenderLink)>,
    world: LinkWorld,
    exports: Query<(), With<BlenderExportRequest>>,
    mut replaced: MessageWriter<PrefabModelReplaced>,
) {
    if !exports.is_empty() {
        return;
    }
    let Ok((owner, link)) = links.single() else {
        return;
    };
    let Some(server) = &link.server else {
        return;
    };
    let mut replaced_models = Vec::new();
    while let Some(mut request) = server.try_next_request() {
        match request.method.as_str() {
            "load_object" => match begin_load(&world, &mut replaced_models) {
                Ok((object, parts, materials)) => {
                    commands.spawn((
                        ChildOf(owner),
                        BlenderExportRequest(Some(request)),
                        BlenderObjectMaterialLoad {
                            object,
                            parts,
                            materials,
                            deadline: Instant::now() + MATERIAL_LOAD_TIMEOUT,
                        },
                    ));
                    break;
                }
                Err(error) => request.answer(Err(error)),
            },
            "save_as_mod" => {
                let Some(name) = request
                    .params
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                else {
                    request.answer(Err("save_as_mod needs a mod name".to_owned()));
                    continue;
                };
                let directory = world.archives.installation_root().to_owned();
                let replacements = world.archives.live_model_replacements();
                let write = IoTaskPool::get().spawn(async move {
                    let path = mod_archive::write_mod_archive(&directory, &name, &replacements)?;
                    let models = replacements
                        .iter()
                        .map(|(model, _)| model.to_string_lossy())
                        .collect::<Vec<_>>();
                    Ok(json!({ "path": path, "models": models }))
                });
                commands.spawn((
                    ChildOf(owner),
                    BlenderExportRequest(Some(request)),
                    BlenderExportTask(write),
                ));
                break;
            }
            _ => {
                let result = answer(&mut request, &world, &mut replaced_models);
                request.answer(result);
            }
        }
    }
    announce(&mut replaced, replaced_models);
}

fn export_blender_objects_with_ready_materials(
    mut commands: Commands,
    mut waiting: Query<(Entity, &mut BlenderObjectMaterialLoad)>,
    mut material_events: MessageReader<AssetEvent<MaterialAsset>>,
    mut material_failures: MessageReader<AssetLoadFailedEvent<MaterialAsset>>,
    materials: Res<Assets<MaterialAsset>>,
    server: Res<AssetServer>,
    archives: Res<AssetArchives>,
    paths: Option<Res<PersistenceFilesystemPaths>>,
) {
    let materials_changed = !material_events.is_empty() || !material_failures.is_empty();
    material_events.clear();
    material_failures.clear();
    let now = Instant::now();
    for (entity, mut load) in &mut waiting {
        if now < load.deadline
            && ((!load.is_added() && !materials_changed)
                || !load.materials.iter().all(|(_, material)| {
                    materials.contains(material) || server.load_state(material).is_failed()
                }))
        {
            continue;
        }
        let Some(paths) = &paths else {
            commands
                .entity(entity)
                .remove::<BlenderObjectMaterialLoad>()
                .insert(BlenderExportTask(IoTaskPool::get().spawn(async {
                    Err("the game has no folder to write models to".to_owned())
                })));
            continue;
        };
        let textures = Arc::new(
            load.materials
                .iter()
                .filter_map(|(path, handle)| {
                    let texture = materials.get(handle)?.first_bound_texture_asset()?;
                    Some((
                        path.clone(),
                        texture.path()?.path().to_string_lossy().into_owned(),
                    ))
                })
                .collect::<BTreeMap<_, _>>(),
        );
        let directory = paths.blender_model_export_directory_path();
        let archives = (*archives).clone();
        let parts = std::mem::take(&mut load.parts);
        let object = std::mem::take(&mut load.object);
        let write = IoTaskPool::get().spawn(async move {
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let mut exported = Vec::with_capacity(parts.len());
            for part in parts {
                let ObjectPart {
                    model,
                    transform,
                    visible,
                    ..
                } = part;
                let path = blender_model_export_path(&directory, &model);
                let archives = archives.clone();
                let textures = Arc::clone(&textures);
                let (model, glb) = AsyncComputeTaskPool::get()
                    .spawn(async move {
                        let glb = encode_editable_gltf_model(&archives, &model, |_, path| {
                            textures.get(path).cloned()
                        })
                        .and_then(|glb| {
                            place_gltf_scene(
                                &glb,
                                transform.translation.to_array(),
                                transform.rotation.to_array(),
                                transform.scale.to_array(),
                            )
                        })
                        .map_err(|error| format!("{model} could not be exported: {error:#}"))?;
                        Ok::<_, String>((model, glb))
                    })
                    .await?;
                std::fs::write(&path, glb).map_err(|error| error.to_string())?;
                exported.push(json!({ "model": model, "path": path, "visible": visible }));
            }
            Ok(json!({ "object": object, "parts": exported }))
        });
        commands
            .entity(entity)
            .remove::<BlenderObjectMaterialLoad>()
            .insert(BlenderExportTask(write));
    }
}

fn blender_model_export_path(directory: &Path, model: &str) -> PathBuf {
    directory.join(format!(
        "{}.glb",
        Path::new(model)
            .with_extension("")
            .to_string_lossy()
            .replace(['/', '\\'], "_")
    ))
}

fn answer_completed_blender_exports(
    mut commands: Commands,
    mut exports: Query<(Entity, &mut BlenderExportRequest, &mut BlenderExportTask)>,
) {
    for (entity, mut request, mut task) in &mut exports {
        if let Some(result) = block_on(poll_once(&mut task.0)) {
            if let Some(request) = request.0.take() {
                request.answer(result);
            }
            commands.entity(entity).despawn();
        }
    }
}

/// The selected object's name and its models. An object is a small scene: a
/// building has its own model, built onto the object, and attached parts
/// such as components and states.
fn selected_object(world: &LinkWorld) -> Option<(String, Vec<ObjectPart>)> {
    let selected = world.selected.subject()?;
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

type BegunLoad = (
    String,
    Vec<ObjectPart>,
    Vec<(String, Handle<MaterialAsset>)>,
);

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

/// Generated native material paths use '#' as Bevy's label delimiter; a
/// `.bfmat` file name may contain '#' itself.
fn material_asset_path(material_path: &str) -> AssetPath<'static> {
    if material_path.to_ascii_lowercase().ends_with(".bfmat") {
        AssetPath::from_path_buf(material_path.into())
    } else {
        AssetPath::from(material_path.to_owned())
    }
}

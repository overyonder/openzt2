use bevy::{
    asset::RenderAssetUsages,
    camera::{
        visibility::RenderLayers, ClearColorConfig, PerspectiveProjection, RenderTarget,
        SubCameraView,
    },
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
};
use openzt2_game_data::{
    ui_document::node_property_binding::UiImagePropertyBindingSource,
    world_definitions::world_objects::WorldObjectKind, AssetId,
};

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_ui::authored_ui_image_content_binding::UiImageBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_world_state::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;

use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;

pub(super) const CATALOGUE_PREVIEW_SCENE_RENDER_LAYER: usize = 29;

#[cfg(test)]
mod tests;
// `buyinfo.xml` defines this exact sub-view in the original 1024-by-768 UI
// surface. The native preview stays attached to `buy_panel_fist` in
// `cameraobject.xml`; it is not reframed from each selected mesh's bounds.
const AUTHORED_BUY_INFORMATION_VIEW_SIZE: UVec2 = UVec2::new(1024, 768);
const AUTHORED_BUY_INFORMATION_PREVIEW_OFFSET: Vec2 = Vec2::new(727.0, 624.0);
const AUTHORED_BUY_INFORMATION_PREVIEW_SIZE: UVec2 = UVec2::new(97, 85);
const AUTHORED_BUY_PANEL_CAMERA_RELATIVE_ANCHOR: Vec3 = Vec3::new(19.5, -7.85, 6.37);

#[derive(Component)]
pub struct CataloguePreviewScene {
    definition: AssetId,
    catalogue_revision: u64,
    scene: Entity,
}

impl CataloguePreviewScene {
    pub fn definition(&self) -> AssetId {
        self.definition
    }

    pub fn scene(&self) -> Entity {
        self.scene
    }
}

#[derive(Component)]
pub(crate) struct CataloguePreviewPrefab(Handle<ScenePrefabAsset>);

impl CataloguePreviewPrefab {
    pub(super) fn new(prefab: Handle<ScenePrefabAsset>) -> Self {
        Self(prefab)
    }
}

#[derive(Component)]
pub struct CataloguePreviewOwner(Entity);

#[derive(Component)]
pub(crate) struct CataloguePreviewRenderable;

#[derive(Component)]
pub(crate) struct CataloguePreviewAnimationAttachmentConsidered;

/// Hosts the selected canonical prefab in an isolated Bevy render target used
/// by the authored buy-details panel. The scene is presentation-only: the
/// selected definition remains the sole catalogue state and no gameplay
/// entity, physics body, or copied object record is created.
pub(crate) fn project_selected_catalogue_entry_into_preview_scene(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    selected_entry: Res<SelectedCatalogueEntry>,
    mut nodes: Query<(
        Entity,
        &UiDocumentOwner,
        &UiImageBinding,
        &InheritedVisibility,
        Option<&CataloguePreviewScene>,
        Option<&mut ImageNode>,
    )>,
) {
    let definitions = active_definitions.get(&definitions);
    for (node, _owner, binding, inherited_visibility, current, image_node) in &mut nodes {
        if !matches!(
            &binding.0,
            UiImagePropertyBindingSource::CatalogueEntryPreview
        ) {
            continue;
        }
        let mut image_node = image_node;
        let definition = selected_entry.0;
        let Some(definition) = definition else {
            if let Some(current) = current {
                commands.entity(current.scene).despawn();
                commands.entity(node).remove::<CataloguePreviewScene>();
            }
            if let Some(image_node) = image_node.as_deref_mut() {
                image_node.image = Handle::default();
            }
            continue;
        };
        if definitions.is_some()
            && current.is_some_and(|current| {
                current.definition == definition
                    && current.catalogue_revision == active_definitions.catalogue_revision()
            })
        {
            continue;
        }
        if let Some(current) = current {
            commands.entity(current.scene).despawn();
            commands.entity(node).remove::<CataloguePreviewScene>();
        }
        if let Some(image_node) = image_node.as_deref_mut() {
            image_node.image = Handle::default();
        }
        let Some(definitions) = definitions else {
            continue;
        };
        let Some(object) = definitions.find_object(definition) else {
            if let Some(image_node) = image_node.as_deref_mut() {
                image_node.image = Handle::default();
            }
            continue;
        };
        let prefab = object.catalogue_preview_prefab.or_else(|| {
            (object.prefab != AssetId::default())
                .then_some(object.prefab)
                .or_else(|| {
                    definitions.find_fence(definition).map(|fence| {
                        (object.kind == WorldObjectKind::Gate
                            && fence.gate_policy.prefab != AssetId::default())
                        .then_some(fence.gate_policy.prefab)
                        .unwrap_or(fence.segments.cardinal_straight)
                    })
                })
                .or_else(|| {
                    object
                        .named_physical_presentations
                        .iter()
                        .find(|presentation| {
                            presentation.required && presentation.prefab != AssetId::default()
                        })
                        .map(|presentation| presentation.prefab)
                })
        });
        let ground_path = definitions
            .find_path(definition)
            .filter(|path| !path.elevated);
        let path_texture =
            ground_path.and_then(|path| definitions.texture_image(path.surface_texture));
        let prefab = prefab.and_then(|prefab| definitions.scene(prefab));
        if prefab.is_none() && path_texture.is_none() {
            if let Some(image_node) = image_node.as_deref_mut() {
                image_node.image = Handle::default();
            }
            continue;
        }
        let mut target = Image::new_target_texture(
            AUTHORED_BUY_INFORMATION_PREVIEW_SIZE.x,
            AUTHORED_BUY_INFORMATION_PREVIEW_SIZE.y,
            TextureFormat::Rgba8UnormSrgb,
            None,
        );
        target.asset_usage = RenderAssetUsages::all();
        target.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING;
        let target = images.add(target);
        let scene = commands
            .spawn((
                Name::new("catalogue object preview scene"),
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
                CataloguePreviewOwner(node),
            ))
            .id();
        let offset = object
            .preview_offset_cm
            .map(|value| f32::from(value) * 0.01);
        if let (Some(path), Some(texture)) = (ground_path, path_texture) {
            super::catalogue_ground_path_preview_construction::spawn_ground_path_catalogue_preview(
                &mut commands,
                scene,
                path,
                definitions,
                texture,
                &mut meshes,
                &mut materials,
            );
        } else if let Some(prefab) = prefab {
            let mut preview = commands.spawn((
                Name::new("catalogue object preview prefab"),
                CataloguePreviewPrefab(prefab),
                Transform::from_translation(
                    AUTHORED_BUY_PANEL_CAMERA_RELATIVE_ANCHOR
                        + Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                            offset,
                        )),
                )
                .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(object.preview_scale * object.prefab_scale)),
                Visibility::Inherited,
                ChildOf(scene),
            ));
            if object.catalogue_preview_prefab.is_none() {
                preview.insert(PrefabObjectPresentationAttachmentProjection(definition));
            }
        }
        commands.spawn((
            Name::new("catalogue object preview light"),
            DirectionalLight {
                illuminance: light_consts::lux::DIRECT_SUNLIGHT,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(-3.0, 4.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
            RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
            ChildOf(scene),
        ));
        commands.spawn((
            Name::new("catalogue object preview camera"),
            Camera3d::default(),
            AmbientLight {
                color: Color::WHITE,
                brightness: 1.0,
                affects_lightmapped_meshes: true,
            },
            Camera {
                is_active: inherited_visibility.get(),
                clear_color: ClearColorConfig::Custom(Color::NONE),
                sub_camera_view: ground_path.is_none().then_some(SubCameraView {
                    full_size: AUTHORED_BUY_INFORMATION_VIEW_SIZE,
                    offset: AUTHORED_BUY_INFORMATION_PREVIEW_OFFSET,
                    size: AUTHORED_BUY_INFORMATION_PREVIEW_SIZE,
                }),
                ..default()
            },
            if let Some(path) = ground_path {
                Projection::Orthographic(OrthographicProjection {
                    scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                        viewport_height: f32::from(path.width_cm) * 0.03,
                    },
                    ..OrthographicProjection::default_3d()
                })
            } else {
                Projection::Perspective(PerspectiveProjection {
                    // The winning overhead camera authors frustum slopes of
                    // -0.48 and +0.48 at a fixed 4:3 aspect.
                    fov: 2.0 * 0.48_f32.atan(),
                    aspect_ratio: 4.0 / 3.0,
                    near: 0.3,
                    far: 800.0,
                    ..default()
                })
            },
            RenderTarget::Image(target.clone().into()),
            if ground_path.is_some() {
                Transform::from_xyz(8.0, 10.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y)
            } else {
                Transform::IDENTITY.looking_to(Vec3::X, Vec3::Y)
            },
            RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
            CataloguePreviewOwner(node),
            ChildOf(scene),
        ));
        if let Some(mut image_node) = image_node {
            image_node.image = target.clone();
            image_node.rect = None;
            image_node.image_mode = NodeImageMode::Stretch;
        } else {
            let mut image_node = ImageNode::new(target.clone());
            image_node.image_mode = NodeImageMode::Stretch;
            commands.entity(node).insert(image_node);
        }
        commands.entity(node).insert(CataloguePreviewScene {
            definition,
            scene,
            catalogue_revision: active_definitions.catalogue_revision(),
        });
    }
}

pub(crate) fn synchronize_catalogue_preview_camera_activity_with_authored_image_visibility(
    owners: Query<&InheritedVisibility>,
    mut cameras: Query<(&CataloguePreviewOwner, &mut Camera)>,
) {
    for (owner, mut camera) in &mut cameras {
        let is_active = owners.get(owner.0).is_ok_and(|visibility| visibility.get());
        if camera.is_active != is_active {
            camera.is_active = is_active;
        }
    }
}

pub(crate) fn hydrate_catalogue_preview_scene_prefab_render_trees(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    requests: Query<(Entity, &CataloguePreviewPrefab)>,
) {
    for (entity, request) in &requests {
        let Some(prefab) = prefabs.get(&request.0) else {
            continue;
        };
        let (_, renderables, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
        for renderable in renderables {
            commands.entity(renderable).insert((
                RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
                CataloguePreviewRenderable,
            ));
        }
        commands.entity(entity).remove::<CataloguePreviewPrefab>();
    }
}

pub(crate) fn attach_loaded_species_animation_to_catalogue_preview_renderables(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    renderables: Query<
        (Entity, &PrefabModel),
        (
            With<CataloguePreviewRenderable>,
            Without<CataloguePreviewAnimationAttachmentConsidered>,
        ),
    >,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    for (renderable, model) in &renderables {
        let mut renderable_commands = commands.entity(renderable);
        renderable_commands.insert(CataloguePreviewAnimationAttachmentConsidered);
        let Some(variant) = species.find_variant_by_model_path(&model.model_path) else {
            continue;
        };
        let Some(animation_set) = species.load_variant_animation_set(&asset_server, variant) else {
            continue;
        };
        // Native animal clips live beside the model rather than inside its
        // generated glTF, which may already have settled as animationless.
        renderable_commands
            .remove::<ModelAnimationAttachmentResolved>()
            .insert(PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: vec![animation_set].into_boxed_slice(),
                requested_initial_animation_clip_asset_key: variant
                    .initial_animation_clip_asset_key
                    .clone(),
            });
    }
}

pub(crate) fn apply_authored_catalogue_preview_initial_animation_repetition_policy_after_attachment(
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    newly_resolved_animation_controllers: Query<
        (Entity, &PrefabModel, &AnimationPlaybackController),
        (
            With<CataloguePreviewRenderable>,
            Added<ModelAnimationAttachmentResolved>,
        ),
    >,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(species_index) = species_index.get(&species_assets) else {
        return;
    };
    for (animation_controller_entity, prefab_model, _) in &newly_resolved_animation_controllers {
        let Some(variant) = species_index.find_variant_by_model_path(&prefab_model.model_path)
        else {
            continue;
        };
        let Some(animation_clip_asset_key) = variant.initial_animation_clip_asset_key.as_ref()
        else {
            continue;
        };
        animation_clip_playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: animation_controller_entity,
            animation_clip_asset_key: animation_clip_asset_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: if variant.initial_animation_loops {
                AnimationPlaybackRepetitionPolicy::Loop
            } else {
                AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
            },
        });
    }
}

pub(crate) fn retire_catalogue_preview_scenes_without_ui_owners(
    mut commands: Commands,
    scenes: Query<(Entity, &CataloguePreviewOwner), (Without<Camera>, Allow<Disabled>)>,
    owners: Query<(), (With<CataloguePreviewScene>, Allow<Disabled>)>,
) {
    // Cameras keep the same owner marker for activity and diagnostics. The
    // scene root alone retires their linked hierarchy.
    for (scene, owner) in &scenes {
        if owners.get(owner.0).is_err() {
            commands.entity(scene).despawn();
        }
    }
}

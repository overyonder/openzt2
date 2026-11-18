use crate::model_render::world_entity_model_suppression::ModelRenderVisibilityGate;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_data::scene_prefab::ScenePrefabEntityFlags;
use openzt2_game_data::scene_prefab::ScenePrefabRenderableVisibilityFlags;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::world_spawn::prefab_model_replacement::PrefabModelReplaced;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabLocalRenderBound;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;

use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use openzt2_world_state::world_spawn::prefab_authored_rotation_cycle_advancement::PrefabAuthoredRotationCycle;
use openzt2_world_state::world_spawn::prefab_authored_transform_animation_advancement::PrefabAuthoredTransformAnimation;
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;

/// Spawn a prefab for isolated render scenes, such as the shell globe.
pub fn spawn_prefab_render_tree(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    parent: Entity,
    authored_lights: bool,
) -> (Entity, Vec<Entity>, Vec<Entity>) {
    let document = prefab.canonical_scene_prefab_document();
    let entities = document
        .entities
        .iter()
        .enumerate()
        .map(|(_index, row)| {
            let transform = transform_from_authored(&row.transform);
            let mut entity = commands.spawn((
                PrefabAuthoredAttachmentIdentifier(AssetId(row.attachment_id.0)),
                transform,
                if row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            if let Some(bound) = row.local_render_bound {
                entity.insert(PrefabLocalRenderBound(bound));
            }
            if let Some(bound) = &row.skin_render_bound {
                entity.insert(openzt2_world_state::world_spawn::prefab_presentation_types::PrefabSkinRenderBound(bound.clone()));
            }
            if let Some(cycle) = row.rotation_cycles.first() {
                entity.insert(PrefabAuthoredRotationCycle::from_authored_rotation_cycle(
                    cycle,
                ));
            }
            if let Some(animation) = row.transform_animations.first() {
                entity.insert(
                    PrefabAuthoredTransformAnimation::from_authored_transform_animation(animation),
                );
            }
            if let Some(billboard) = row.billboards.first() {
                entity.insert(
                    PrefabAuthoredBillboardOrientationMode::from_authored_mode_and_local_rotation(
                        billboard.mode,
                        transform.rotation,
                    )
                    .facing_positive_z(billboard.faces_positive_z),
                );
            }
            entity.id()
        })
        .collect::<Vec<_>>();
    let root = entities
        .first()
        .copied()
        .expect("scene prefab asset must contain a root entity");
    commands.entity(root).insert(ChildOf(parent));
    let mut renderables = Vec::new();
    let mut lights = Vec::new();
    for (index, row) in document.entities.iter().enumerate() {
        let spawned = entities[index];
        if let Some(joint) = &row.model_joint_binding {
            commands.entity(spawned).insert(
                openzt2_world_state::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment::new(root, joint),
            );
        }
        for renderable in &row.renderables {
            let renderable_components =
                crate::world_spawn::prefab_renderable_components::create_prefab_renderable_components(
                    prefab, renderable,
                );
            let renderable_is_the_authored_entity = row.renderables.len() == 1
                && renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0 != 0;
            let mut renderable_entity = if renderable_is_the_authored_entity {
                let mut authored_entity = commands.entity(spawned);
                authored_entity.insert(renderable_components);
                authored_entity
            } else {
                let mut child = commands.spawn((
                    Transform::IDENTITY,
                    if renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0
                        != 0
                    {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    ChildOf(spawned),
                ));
                child.insert(renderable_components);
                child
            };
            let animations = prefab
                .loaded_animation_set_asset_handles()
                .cloned()
                .collect::<Vec<_>>();
            if !animations.is_empty() {
                renderable_entity.insert(PendingModelAnimationAssets {
                    pending_model_animation_set_asset_handles: animations.into_boxed_slice(),
                    requested_initial_animation_clip_asset_key: None,
                });
            }
            let renderable = renderable_entity.id();
            renderables.push(renderable);
        }
        for light in row.lights.iter().filter(|_| authored_lights) {
            let color = Color::srgb(
                light.color_srgb[0],
                light.color_srgb[1],
                light.color_srgb[2],
            );
            match light.kind {
                openzt2_game_data::scene_prefab::PrefabLightKind::Ambient => {}
                openzt2_game_data::scene_prefab::PrefabLightKind::Directional => {
                    let entity = commands
                        .spawn((
                            ModelRenderVisibilityGate,
                            ChildOf(spawned),
                            DirectionalLight {
                                color,
                                // The fixed-function shader maps DIRECT_SUNLIGHT
                                // back to the authored unit directional intensity.
                                illuminance: light.intensity * light_consts::lux::DIRECT_SUNLIGHT,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                        ))
                        .id();
                    lights.push(entity);
                }
                openzt2_game_data::scene_prefab::PrefabLightKind::Point => {
                    let entity = commands
                        .spawn((
                            ModelRenderVisibilityGate,
                            ChildOf(spawned),
                            PointLight {
                                color,
                                intensity: light.intensity,
                                range: light.range_m,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                        ))
                        .id();
                    lights.push(entity);
                }
            }
        }
        for child in &row.children {
            commands
                .entity(entities[*child as usize])
                .insert(ChildOf(spawned));
        }
    }
    for (entity, row) in document.entities.iter().enumerate() {
        for lod in &row.lods {
            commands
                .entity(entities[entity])
                .insert(PrefabModelLevelOfDetailVisibilityRange {
                    group: entities[lod.group_entity as usize],
                    ordinal: lod.ordinal,
                    center_m: Vec3::from_array(lod.center_m),
                    near_m: lod.near_m,
                    far_m: lod.far_m,
                    active_without_range: lod.active_without_range,
                });
        }
    }
    (root, renderables, lights)
}

#[derive(Component)]
#[relationship(relationship_target = PrefabPresentationRenderRoot)]
pub(super) struct PrefabPresentationRenderOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PrefabPresentationRenderOf, linked_spawn)]
pub(super) struct PrefabPresentationRenderRoot(Entity);

pub(super) fn hydrate_prefab_presentations(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    pending: Query<
        (Entity, &PrefabPresentation, Option<&PrefabShadowPolicy>),
        Without<PrefabPresentationHydrated>,
    >,
) {
    for (entity, request, inherited_shadow_policy) in &pending {
        let Some(prefab) = prefabs.get(&request.0) else {
            continue;
        };
        let (root, renderables, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
        for renderable in renderables {
            let mut renderable_commands = commands.entity(renderable);
            if let Some(shadow_policy) = inherited_shadow_policy {
                renderable_commands.insert(*shadow_policy);
            }
        }
        commands
            .entity(root)
            .insert(PrefabPresentationRenderOf(entity));
        commands.entity(entity).insert(PrefabPresentationHydrated);
    }
}

#[derive(Message)]
pub(super) struct ReplacedPrefabReloaded(pub(super) bevy::asset::AssetId<ScenePrefabAsset>);

pub(super) fn publish_replaced_prefab_reloads(
    mut replacements: MessageReader<PrefabModelReplaced>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut reloaded: MessageWriter<ReplacedPrefabReloaded>,
    asset_server: Res<AssetServer>,
    mut pending_paths: Local<HashSet<String>>,
) {
    pending_paths.extend(
        replacements
            .read()
            .map(|event| event.model_path.to_ascii_lowercase()),
    );
    for event in prefab_events.read() {
        let AssetEvent::Modified { id } = *event else {
            continue;
        };
        if pending_paths.is_empty() {
            continue;
        }
        if let Some(path) = asset_server.get_path(id) {
            if pending_paths.remove(&path.path().to_string_lossy().to_ascii_lowercase()) {
                reloaded.write(ReplacedPrefabReloaded(id));
            }
        }
    }
}

pub(super) fn rebuild_prefab_presentations_after_prefab_reload(
    mut commands: Commands,
    mut reloaded: MessageReader<ReplacedPrefabReloaded>,
    presentations: Query<(Entity, &PrefabPresentation, &PrefabPresentationRenderRoot)>,
) {
    let reloaded = reloaded.read().map(|event| event.0).collect::<HashSet<_>>();
    if reloaded.is_empty() {
        return;
    }
    for (entity, presentation, render_root) in &presentations {
        if reloaded.contains(&presentation.0.id()) {
            commands.entity(render_root.0).try_despawn();
            commands
                .entity(entity)
                .remove::<PrefabPresentationHydrated>();
        }
    }
}

use bevy::prelude::*;
use openzt2_game_data::{
    scene_prefab::{ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags},
    AssetId,
};

use crate::{
    assets::scene_prefab::ScenePrefabAsset,
    plugins::{
        animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets,
        world_spawn::prefab_presentation_types::{
            PrefabPresentation, PrefabPresentationHydrated, PrefabShadowPolicy,
        },
    },
};

use super::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use super::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use super::prefab_authored_rotation_cycle_advancement::PrefabAuthoredRotationCycle;
use super::prefab_authored_transform_animation_advancement::PrefabAuthoredTransformAnimation;
use super::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange;
use super::prefab_transform_conversion::transform_from_authored;
use super::world_loading_performance_attribution::{
    WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
};

/// Spawn a prefab for isolated render scenes, such as the shell globe.
pub(crate) fn spawn_prefab_render_tree(
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
                    ),
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
                crate::plugins::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment::new(root, joint),
            );
        }
        for renderable in &row.renderables {
            let renderable_components =
                super::prefab_renderable_components::create_prefab_renderable_components(
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
                    commands.entity(spawned).insert(DirectionalLight {
                        color,
                        // The fixed-function shader maps DIRECT_SUNLIGHT
                        // back to the authored unit directional intensity.
                        illuminance: light.intensity * light_consts::lux::DIRECT_SUNLIGHT,
                        shadow_maps_enabled: false,
                        ..default()
                    });
                    lights.push(spawned);
                }
                openzt2_game_data::scene_prefab::PrefabLightKind::Point => {
                    commands.entity(spawned).insert(PointLight {
                        color,
                        intensity: light.intensity,
                        range: light.range_m,
                        shadow_maps_enabled: false,
                        ..default()
                    });
                    lights.push(spawned);
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

pub(super) fn hydrate_prefab_presentations(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    pending: Query<
        (Entity, &PrefabPresentation, Option<&PrefabShadowPolicy>),
        Without<PrefabPresentationHydrated>,
    >,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::PrefabPresentationHydration);
    for (entity, request, inherited_shadow_policy) in &pending {
        let Some(prefab) = prefabs.get(&request.0) else {
            continue;
        };
        let (_, renderables, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
        for renderable in renderables {
            let mut renderable_commands = commands.entity(renderable);
            if let Some(shadow_policy) = inherited_shadow_policy {
                renderable_commands.insert(*shadow_policy);
            }
        }
        commands.entity(entity).insert(PrefabPresentationHydrated);
    }
}

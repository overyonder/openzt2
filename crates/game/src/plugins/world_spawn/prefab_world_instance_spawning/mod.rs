use avian3d::prelude::RigidBody;
use bevy::{
    ecs::entity_disabling::Disabled,
    prelude::{
        default, ChildOf, Color, Commands, Entity, Handle, PointLight, Transform, Visibility,
    },
};
use openzt2_game_data::{
    scene_prefab::{PrefabLightKind, ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags},
    AssetId,
};

use crate::{
    assets::scene_prefab::ScenePrefabAsset,
    plugins::{
        animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets,
        information::entity_selection_types::Inspectable,
        physics::collider_hydration::hydrate_scene_prefab_colliders_into_avian_physics,
    },
};

use super::{
    persistent_id_types::PersistentId,
    prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode,
    prefab_authored_rotation_cycle_advancement::PrefabAuthoredRotationCycle,
    prefab_authored_transform_animation_advancement::PrefabAuthoredTransformAnimation,
    prefab_effect_trigger_hydration::spawn_prefab_effect_triggers,
    prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange,
    prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection,
    prefab_source_asset_handle::PrefabSourceAssetHandle,
    prefab_transform_conversion::transform_from_authored,
    world_membership_types::{DefinitionId, WorldMember},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_loaded_scene_prefab_as_world_instance(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    prefab_handle: Handle<ScenePrefabAsset>,
    world_root: Entity,
    definition: AssetId,
    persistent_id: PersistentId,
    instance_transform: Transform,
    instance_visible: bool,
    instance_parent: Option<Entity>,
    prefab_collider_rigid_body: RigidBody,
) -> Entity {
    let root = commands.spawn_empty().id();
    hydrate_loaded_scene_prefab_into_world_instance_root(
        commands,
        root,
        prefab,
        prefab_handle,
        world_root,
        definition,
        persistent_id,
        instance_transform,
        instance_visible,
        instance_parent,
        prefab_collider_rigid_body,
    );
    root
}

/// Hydrate a newly admitted world entity without replacing its live identity.
/// The root must not already own a hydrated prefab hierarchy.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hydrate_loaded_scene_prefab_into_world_instance_root(
    commands: &mut Commands,
    root: Entity,
    prefab: &ScenePrefabAsset,
    prefab_handle: Handle<ScenePrefabAsset>,
    world_root: Entity,
    definition: AssetId,
    persistent_id: PersistentId,
    instance_transform: Transform,
    instance_visible: bool,
    instance_parent: Option<Entity>,
    prefab_collider_rigid_body: RigidBody,
) {
    let document = prefab.canonical_scene_prefab_document();
    let mut entities = Vec::with_capacity(document.entities.len());

    for (index, row) in document.entities.iter().enumerate() {
        let local = transform_from_authored(&row.transform);
        let transform = if index == 0 {
            instance_transform.mul_transform(local)
        } else {
            local
        };
        let visible = instance_visible && row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0;
        let mut entity = if index == 0 {
            commands.entity(root)
        } else {
            commands.spawn_empty()
        };
        entity.insert((
            WorldMember { root: world_root },
            PrefabAuthoredAttachmentIdentifier(AssetId(row.attachment_id.0)),
            transform,
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
        let spawned = entity.id();
        if row.flags.0 & ScenePrefabEntityFlags::ACTIVE.0 == 0 {
            entity.insert(Disabled);
        }
        if index == 0 {
            entity.insert((
                DefinitionId(definition),
                persistent_id,
                PrefabObjectPresentationAttachmentProjection(definition),
            ));
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
                ),
            );
        }
        if let Some(light) = row.lights.first() {
            let color = Color::srgb(
                light.color_srgb[0],
                light.color_srgb[1],
                light.color_srgb[2],
            );
            let intensity = light.intensity;
            let range = light.range_m;
            match &light.kind {
                // NIF directional and ambient effects are scoped to their
                // authored subtree. Promoting every imported studio rig to a
                // world light makes each object illuminate every other
                // object. The environment plugin is the Bevy owner of
                // world-wide directional and ambient lighting.
                PrefabLightKind::Directional | PrefabLightKind::Ambient => {}
                PrefabLightKind::Point => {
                    entity.insert(PointLight {
                        color,
                        intensity,
                        range,
                        ..default()
                    });
                }
            }
        }
        if index == 0 {
            entity.insert(Inspectable { definition });
        }
        drop(entity);
        for renderable in &row.renderables {
            let renderable_visible = visible
                && renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0 != 0;
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
                    WorldMember { root: world_root },
                    Transform::IDENTITY,
                    if renderable_visible {
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
        }
        entities.push(spawned);
    }

    commands
        .entity(entities[0])
        .insert(PrefabSourceAssetHandle(prefab_handle.clone()));

    for (entity, row) in document.entities.iter().enumerate() {
        if let Some(joint) = &row.model_joint_binding {
            commands.entity(entities[entity]).insert(
                crate::plugins::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment::new(entities[0], joint),
            );
        }
        for lod in &row.lods {
            commands
                .entity(entities[entity])
                .insert(PrefabModelLevelOfDetailVisibilityRange {
                    group: entities[lod.group_entity as usize],
                    ordinal: lod.ordinal,
                    center_m: bevy::math::Vec3::from_array(lod.center_m),
                    near_m: lod.near_m,
                    far_m: lod.far_m,
                    active_without_range: lod.active_without_range,
                });
        }
    }

    // Preserve the prefab's parent-child hierarchy.
    if let Some(parent) = instance_parent {
        commands.entity(entities[0]).insert(ChildOf(parent));
    }
    for (parent_index, row) in document.entities.iter().enumerate() {
        for child_index in &row.children {
            commands
                .entity(entities[*child_index as usize])
                .insert(ChildOf(entities[parent_index]));
        }
    }

    hydrate_scene_prefab_colliders_into_avian_physics(
        commands,
        prefab,
        &entities,
        world_root,
        prefab_collider_rigid_body,
    );

    spawn_prefab_effect_triggers(commands, prefab, &entities, world_root);
}

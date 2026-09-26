use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, prelude::*};

use crate::{
    assets::scene_prefab::ScenePrefabAsset,
    plugins::world_spawn::{
        persistent_id_types::PersistentId,
        prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
        prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
        world_membership_types::WorldRoot,
    },
};

use super::immersive_mode_policy_types::{InteractionPrefab, InteractionPrefabVisual};

/// Keeps the current visual until its replacement prefab and models are loaded.
pub(super) fn create_world_prefab_visual_when_immersive_mode_prefab_is_ready(
    prefab_policies: Query<(Entity, &InteractionPrefab)>,
    scene_prefab_assets: Res<Assets<ScenePrefabAsset>>,
    gltf_assets: Res<Assets<Gltf>>,
    world_roots: Query<Entity, With<WorldRoot>>,
    existing_prefab_visuals: Query<(Entity, &InteractionPrefabVisual)>,
    mut commands: Commands,
) {
    for (controller_entity, interaction_prefab) in &prefab_policies {
        if existing_prefab_visuals.iter().any(|(_, visual)| {
            visual.owner == controller_entity && visual.prefab == *interaction_prefab
        }) {
            continue;
        }
        let Some(scene_prefab_asset) = scene_prefab_assets.get(&interaction_prefab.handle) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(scene_prefab_asset, &gltf_assets).is_some()
        {
            continue;
        }
        let Ok(world_root_entity) = world_roots.single() else {
            continue;
        };
        for (prefab_visual_entity, prefab_visual) in &existing_prefab_visuals {
            if prefab_visual.owner == controller_entity {
                commands.entity(prefab_visual_entity).despawn();
            }
        }
        let prefab_root_entity = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            scene_prefab_asset,
            interaction_prefab.handle.clone(),
            world_root_entity,
            interaction_prefab.definition,
            PersistentId(u64::MAX - controller_entity.to_bits()),
            Transform::IDENTITY,
            true,
            None,
            RigidBody::Static,
        );
        commands
            .entity(prefab_root_entity)
            .insert(InteractionPrefabVisual {
                owner: controller_entity,
                prefab: interaction_prefab.clone(),
            });
    }
}

pub(super) fn retire_prefab_visuals_without_immersive_mode_prefab_owner(
    prefab_policy_owners: Query<(), With<InteractionPrefab>>,
    prefab_visuals: Query<(Entity, &InteractionPrefabVisual)>,
    mut commands: Commands,
) {
    for (prefab_visual_entity, prefab_visual) in &prefab_visuals {
        if prefab_policy_owners.get(prefab_visual.owner).is_err() {
            commands.entity(prefab_visual_entity).despawn();
        }
    }
}

pub(super) fn retire_all_immersive_mode_prefab_visuals_when_leaving_gameplay(
    prefab_visuals: Query<Entity, With<InteractionPrefabVisual>>,
    mut commands: Commands,
) {
    for prefab_visual_entity in &prefab_visuals {
        commands.entity(prefab_visual_entity).despawn();
    }
}

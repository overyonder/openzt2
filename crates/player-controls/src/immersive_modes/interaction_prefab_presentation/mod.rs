use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, prelude::*};

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::immersive_mode_policy_types::{
    InteractionPrefab, InteractionPrefabVisual, InteractionPrefabVisuals,
};

/// Keeps the current visual until its replacement prefab and models are loaded.
#[derive(Component)]
pub(super) struct ProjectedImmersivePrefab(Handle<ScenePrefabAsset>);

pub(super) fn create_world_prefab_visual_when_immersive_mode_prefab_is_ready(
    prefab_policies: Query<(
        Entity,
        &InteractionPrefab,
        Option<&InteractionPrefabVisuals>,
    )>,
    scene_prefab_assets: Res<Assets<ScenePrefabAsset>>,
    gltf_assets: Res<Assets<Gltf>>,
    world_roots: Query<Entity, With<WorldRoot>>,
    existing_prefab_visuals: Query<&ProjectedImmersivePrefab>,
    mut commands: Commands,
) {
    for (controller_entity, interaction_prefab, existing) in &prefab_policies {
        if existing
            .into_iter()
            .flat_map(|visuals| visuals.iter())
            .any(|entity| {
                existing_prefab_visuals
                    .get(entity)
                    .is_ok_and(|prefab| prefab.0 == interaction_prefab.handle)
            })
        {
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
        for entity in existing.into_iter().flat_map(|visuals| visuals.iter()) {
            commands.entity(entity).despawn();
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
        commands.entity(prefab_root_entity).insert((
            InteractionPrefabVisual(controller_entity),
            ProjectedImmersivePrefab(interaction_prefab.handle.clone()),
        ));
    }
}

pub(super) fn remove_immersive_prefab_on_policy_removal(
    removed: On<Remove<InteractionPrefab>>,
    children: Query<&InteractionPrefabVisuals>,
    mut commands: Commands,
) {
    if let Ok(children) = children.get(removed.entity) {
        for entity in children.iter() {
            commands.entity(entity).try_despawn();
        }
    }
}

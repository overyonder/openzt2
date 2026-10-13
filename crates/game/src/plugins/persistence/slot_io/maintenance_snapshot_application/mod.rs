use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    maintenance_snapshot_types::HabitatWasteSnapshotRecord,
};
use crate::plugins::{
    information::entity_selection_types::Inspectable,
    maintenance::maintenance_types::AnimalHabitatWaste,
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use avian3d::prelude::RigidBody;
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};
use openzt2_game_data::{world_definitions::world_objects::WorldObjectPropertyFlags, AssetId};

pub(super) fn resolve_habitat_waste_prefabs(
    records: &[HabitatWasteSnapshotRecord],
    definitions: WorldDefinitionsView<'_>,
    prefabs: &Assets<ScenePrefabAsset>,
    server: &AssetServer,
    retained_prefabs: &mut Vec<Handle<ScenePrefabAsset>>,
) -> Result<Option<Vec<Option<Handle<ScenePrefabAsset>>>>, WorldSnapshotPersistenceFailure> {
    let mut resolved = Vec::with_capacity(records.len());
    let mut ready = true;
    for record in records {
        let object = definitions
            .find_object(record.definition)
            .filter(|object| {
                object
                    .properties
                    .contains_all(WorldObjectPropertyFlags::ANIMAL_WASTE)
            })
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        if object.prefab == AssetId::default() {
            resolved.push(None);
            continue;
        }
        let handle = definitions
            .scene(object.prefab)
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        if matches!(
            server.get_load_state(handle.id()),
            Some(LoadState::Failed(_))
        ) || matches!(
            server.get_recursive_dependency_load_state(handle.id()),
            Some(RecursiveDependencyLoadState::Failed(_))
        ) {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
        if !retained_prefabs.contains(&handle) {
            retained_prefabs.push(handle.clone());
        }
        ready &= prefabs.get(&handle).is_some();
        resolved.push(Some(handle));
    }
    Ok(ready.then_some(resolved))
}

pub(super) fn apply_habitat_waste_records(
    commands: &mut Commands,
    records: &[HabitatWasteSnapshotRecord],
    resolved_prefabs: &[Option<Handle<ScenePrefabAsset>>],
    prefabs: &Assets<ScenePrefabAsset>,
    world_root: Entity,
    allocator: &mut PersistentIdAllocator,
    baseline_waste: &Query<Entity, With<AnimalHabitatWaste>>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() != resolved_prefabs.len()
        || resolved_prefabs
            .iter()
            .flatten()
            .any(|handle| prefabs.get(handle).is_none())
    {
        return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
    }
    for record in records {
        allocator
            .reserve_imported(world_root, PersistentId(record.persistent_identifier))
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    }
    for entity in baseline_waste {
        commands.entity(entity).despawn();
    }
    for (record, prefab) in records.iter().zip(resolved_prefabs) {
        let id = PersistentId(record.persistent_identifier);
        let entity = if let Some(handle) = prefab {
            spawn_loaded_scene_prefab_as_world_instance(
                commands,
                prefabs
                    .get(handle)
                    .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?,
                handle.clone(),
                world_root,
                record.definition,
                id,
                Transform::IDENTITY,
                true,
                None,
                RigidBody::Kinematic,
            )
        } else {
            commands
                .spawn((
                    WorldMember { root: world_root },
                    DefinitionId(record.definition),
                    id,
                    Visibility::Inherited,
                    Inspectable {
                        definition: record.definition,
                    },
                ))
                .id()
        };
        // The saved root pose already includes the authored root transform and scale.
        // Insertion fires the cleanliness hook once; Added<AnimalHabitatWaste> rebuilds work.
        commands.entity(entity).insert((
            Transform {
                translation: Vec3::from_array(record.translation),
                rotation: Quat::from_array(record.rotation),
                scale: Vec3::from_array(record.scale),
            },
            AnimalHabitatWaste {
                uncontained_waste_units: record.units,
            },
        ));
    }
    Ok(())
}

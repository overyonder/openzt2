use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    maintenance_snapshot_types::{
        LooseWasteSnapshotKind, LooseWasteSnapshotRecord, MaintainedObjectSnapshotRecord,
    },
};
use avian3d::prelude::RigidBody;
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    ecs::query::QueryData,
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};
use openzt2_game_data::{world_definitions::world_objects::WorldObjectPropertyFlags, AssetId};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::maintenance::maintenance_types::{
    AnimalHabitatWaste, AnimalWasteProductionSchedule, CompostPooLevel, FacilityContainedWaste,
    LooseLitterWaste,
};

pub(super) fn resolve_loose_waste_prefabs(
    records: &[LooseWasteSnapshotRecord],
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
                record.kind == LooseWasteSnapshotKind::Litter
                    || object
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

pub(super) fn apply_loose_waste_records(
    commands: &mut Commands,
    records: &[LooseWasteSnapshotRecord],
    resolved_prefabs: &[Option<Handle<ScenePrefabAsset>>],
    prefabs: &Assets<ScenePrefabAsset>,
    world_root: Entity,
    allocator: &mut PersistentIdAllocator,
    baseline_waste: &Query<Entity, super::maintenance_snapshot_capture::LooseWasteSnapshotFilter>,
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
            .reserve_imported(PersistentId(record.persistent_identifier))
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
        // Insertion fires the cleanliness hook and rebuilds the matching cleanup work.
        let mut restored = commands.entity(entity);
        restored.insert(Transform {
            translation: Vec3::from_array(record.translation),
            rotation: Quat::from_array(record.rotation),
            scale: Vec3::from_array(record.scale),
        });
        match record.kind {
            LooseWasteSnapshotKind::AnimalHabitat => {
                restored.insert(AnimalHabitatWaste {
                    uncontained_waste_units: record.units,
                });
            }
            LooseWasteSnapshotKind::Litter => {
                restored.insert(LooseLitterWaste {
                    uncontained_waste_units: record.units,
                });
            }
        }
    }
    Ok(())
}

/// Restores saved bin contents, compost levels and animal production deadlines. Authored
/// initial facts only fill values that are still absent.
#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredMaintenanceComponents {
    waste: Option<&'static mut FacilityContainedWaste>,
    compost: Option<&'static mut CompostPooLevel>,
    schedule: Option<&'static mut AnimalWasteProductionSchedule>,
}

pub(super) fn apply_maintained_object_records(
    commands: &mut Commands,
    records: &[MaintainedObjectSnapshotRecord],
    identities: &PersistentIdIndex,
    root: Entity,
    objects: &mut Query<RestoredMaintenanceComponents>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    use super::restored_component_values::restore_optional_component;
    for record in records {
        let entity = identities
            .get(root, PersistentId(record.persistent_identifier))
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let live = objects
            .get_mut(entity)
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let mut object = commands.entity(entity);
        let waste = record
            .contained_waste
            .map(|[units, capacity]| FacilityContainedWaste {
                contained_waste_units: units,
                contained_waste_capacity_units: capacity,
            });
        restore_optional_component(&mut object, live.waste, waste, false);
        restore_optional_component(
            &mut object,
            live.compost,
            record.compost_poo_level.map(CompostPooLevel),
            false,
        );
        let schedule = record
            .next_waste_production_tick
            .map(|next_production_tick| AnimalWasteProductionSchedule {
                next_production_tick,
            });
        restore_optional_component(&mut object, live.schedule, schedule, false);
    }
    Ok(())
}

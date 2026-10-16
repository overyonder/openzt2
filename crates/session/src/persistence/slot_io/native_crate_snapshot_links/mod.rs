use std::collections::{BTreeMap, BTreeSet};

use bevy::{ecs::resource::IsResource, prelude::*};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    world_entity_crating::{AnimalCrate, ContainedCratedAnimal, WorldEntityIsCrated},
    world_membership_types::WorldMember,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    animal_snapshot_capture::AnimalSnapshotEntities, animal_snapshot_types::AnimalSnapshotRecord,
    placement_snapshot_section::PlacementSnapshotRecord,
};

pub(super) type NativeCrateSnapshotEntities<'w, 's> =
    Query<'w, 's, EntityRef<'static>, (With<ContainedCratedAnimal>, Without<IsResource>)>;

pub(super) fn capture_native_crate_links(
    animals: &AnimalSnapshotEntities,
    crates: &NativeCrateSnapshotEntities,
    records: &mut [AnimalSnapshotRecord],
) -> Result<(), Failure> {
    for animal in animals {
        let Some(link) = animal.get::<AnimalCrate>() else {
            continue;
        };
        let crate_entity = crates
            .get(link.0)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let crate_id = crate_entity
            .get::<PersistentId>()
            .ok_or(Failure::BrokenPersistentEntityReference)?
            .0;
        let animal_id = animal
            .get::<PersistentId>()
            .ok_or(Failure::BrokenPersistentEntityReference)?
            .0;
        if crate_entity
            .get::<ContainedCratedAnimal>()
            .map(|link| link.0)
            != Some(animal.id())
            || !animal.contains::<WorldEntityIsCrated>()
            || animal.get::<WorldMember>().is_none()
            || animal.get::<WorldMember>() != crate_entity.get::<WorldMember>()
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        let transform = crate_entity
            .get::<Transform>()
            .ok_or(Failure::CorruptSnapshotSection)?;
        let index = records
            .binary_search_by_key(&animal_id, |record| record.persistent_identifier)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let record = &mut records[index];
        record.crate_identifier = Some(crate_id);
        // The crate owns the current pose, including relocation earlier this frame.
        record.translation = transform.translation.to_array();
        record.rotation = transform.rotation.to_array();
    }
    for crate_entity in crates {
        let link = crate_entity
            .get::<ContainedCratedAnimal>()
            .ok_or(Failure::BrokenPersistentEntityReference)?;
        if animals
            .get(link.0)
            .ok()
            .and_then(|animal| animal.get::<AnimalCrate>().map(|link| link.0))
            != Some(crate_entity.id())
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
    }
    Ok(())
}

pub(super) fn validate_native_crate_links(
    animals: &[AnimalSnapshotRecord],
    placements: &[PlacementSnapshotRecord],
) -> Result<(), Failure> {
    let mut linked_crates = BTreeSet::new();
    for animal in animals {
        let Some(crate_id) = animal.crate_identifier else {
            continue;
        };
        if !animal.crated
            || matches!(crate_id, 0 | u64::MAX)
            || !linked_crates.insert(crate_id)
            || animals
                .binary_search_by_key(&crate_id, |record| record.persistent_identifier)
                .is_ok()
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        let index = placements
            .binary_search_by_key(&crate_id, |record| record.persistent_identifier)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let transform = placements[index].transform;
        if animal.translation != transform.translation.to_array()
            || animal.rotation != transform.rotation.to_array()
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn apply_native_crate_links(
    commands: &mut Commands,
    records: &[AnimalSnapshotRecord],
    animals: &BTreeMap<u64, Entity>,
    placements: &BTreeMap<u64, Entity>,
) -> Result<(), Failure> {
    let links = records
        .iter()
        .filter_map(|record| {
            record
                .crate_identifier
                .map(|crate_id| (record.persistent_identifier, crate_id))
        })
        .map(|(animal_id, crate_id)| {
            let animal = animals
                .get(&animal_id)
                .copied()
                .ok_or(Failure::BrokenPersistentEntityReference)?;
            let crate_entity = placements
                .get(&crate_id)
                .copied()
                .ok_or(Failure::BrokenPersistentEntityReference)?;
            if animal == crate_entity {
                return Err(Failure::BrokenPersistentEntityReference);
            }
            Ok((animal, crate_entity))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    for (animal, crate_entity) in links {
        commands.entity(animal).insert((
            AnimalCrate(crate_entity),
            WorldEntityIsCrated,
            Visibility::Hidden,
        ));
        commands
            .entity(crate_entity)
            .insert(ContainedCratedAnimal(animal));
    }
    Ok(())
}

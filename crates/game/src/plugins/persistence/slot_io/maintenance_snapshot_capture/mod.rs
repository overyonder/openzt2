use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    maintenance_snapshot_section_encoding::{
        validate_habitat_waste_records, MAXIMUM_WASTE_RECORDS,
    },
    maintenance_snapshot_types::HabitatWasteSnapshotRecord,
};
use crate::plugins::{
    maintenance::maintenance_types::AnimalHabitatWaste,
    world_spawn::{
        persistent_id_types::PersistentId,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use bevy::{ecs::resource::IsResource, prelude::*};

/// Whole waste entities, excluding resource entities as for animals.
pub(in crate::plugins::persistence) type HabitatWasteSnapshotEntities<'w, 's> =
    Query<'w, 's, EntityRef<'static>, (With<AnimalHabitatWaste>, Without<IsResource>)>;

pub(super) fn capture_habitat_waste_records(
    waste: &HabitatWasteSnapshotEntities,
) -> Result<Vec<HabitatWasteSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let count = waste.iter().len();
    if count > MAXIMUM_WASTE_RECORDS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let mut records = Vec::with_capacity(count);
    for entity in waste {
        let failure = WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference;
        let id = entity.get::<PersistentId>().ok_or(failure)?;
        let definition = entity.get::<DefinitionId>().ok_or(failure)?;
        entity.get::<WorldMember>().ok_or(failure)?;
        // Spawned waste is a world root; parent-relative poses cannot be restored independently.
        if entity.contains::<ChildOf>() {
            return Err(failure);
        }
        let transform = entity.get::<Transform>().ok_or(failure)?;
        let waste = entity.get::<AnimalHabitatWaste>().ok_or(failure)?;
        records.push(HabitatWasteSnapshotRecord {
            persistent_identifier: id.0,
            definition: definition.0,
            translation: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            scale: transform.scale.to_array(),
            units: waste.uncontained_waste_units,
        });
    }
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    validate_habitat_waste_records(&records)?;
    Ok(records)
}

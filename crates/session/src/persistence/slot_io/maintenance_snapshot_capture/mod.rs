use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    maintenance_snapshot_section_encoding::{validate_loose_waste_records, MAXIMUM_WASTE_RECORDS},
    maintenance_snapshot_types::{
        LooseWasteSnapshotKind, LooseWasteSnapshotRecord, MaintainedObjectSnapshotRecord,
    },
};
use bevy::{ecs::resource::IsResource, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::maintenance::maintenance_types::{
    AnimalHabitatWaste, AnimalWasteProductionSchedule, FacilityContainedWaste, LooseLitterWaste,
    MaintainableObjectConditionPermille,
};

pub(in crate::persistence) type LooseWasteSnapshotFilter =
    Or<(With<AnimalHabitatWaste>, With<LooseLitterWaste>)>;

/// Whole waste entities, excluding resource entities as for animals.
pub(in crate::persistence) type LooseWasteSnapshotEntities<'w, 's> =
    Query<'w, 's, EntityRef<'static>, (LooseWasteSnapshotFilter, Without<IsResource>)>;

pub(super) fn capture_loose_waste_records(
    waste: &LooseWasteSnapshotEntities,
) -> Result<Vec<LooseWasteSnapshotRecord>, WorldSnapshotPersistenceFailure> {
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
        let (kind, units) = match (
            entity.get::<AnimalHabitatWaste>(),
            entity.get::<LooseLitterWaste>(),
        ) {
            (Some(waste), None) => (
                LooseWasteSnapshotKind::AnimalHabitat,
                waste.uncontained_waste_units,
            ),
            (None, Some(waste)) => (
                LooseWasteSnapshotKind::Litter,
                waste.uncontained_waste_units,
            ),
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        };
        records.push(LooseWasteSnapshotRecord {
            kind,
            persistent_identifier: id.0,
            definition: definition.0,
            translation: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            scale: transform.scale.to_array(),
            units,
        });
    }
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    validate_loose_waste_records(&records)?;
    Ok(records)
}

/// Entities carrying worn condition, retained bin waste or waste-production deadlines.
pub(in crate::persistence) type MaintainedObjectSnapshotEntities<'w, 's> = Query<
    'w,
    's,
    (
        &'static PersistentId,
        Option<&'static MaintainableObjectConditionPermille>,
        Option<&'static FacilityContainedWaste>,
        Option<&'static AnimalWasteProductionSchedule>,
    ),
    (
        Or<(
            With<MaintainableObjectConditionPermille>,
            With<FacilityContainedWaste>,
            With<AnimalWasteProductionSchedule>,
        )>,
        Without<IsResource>,
    ),
>;

pub(super) fn capture_maintained_object_records(
    objects: &MaintainedObjectSnapshotEntities,
) -> Vec<MaintainedObjectSnapshotRecord> {
    let mut records = objects
        .iter()
        .map(
            |(id, condition, waste, production)| MaintainedObjectSnapshotRecord {
                persistent_identifier: id.0,
                condition_permille: condition.map(|condition| condition.0),
                contained_waste: waste.map(|waste| {
                    [
                        waste.contained_waste_units,
                        waste.contained_waste_capacity_units,
                    ]
                }),
                next_waste_production_tick: production
                    .map(|production| production.next_production_tick),
            },
        )
        .collect::<Vec<_>>();
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    records
}

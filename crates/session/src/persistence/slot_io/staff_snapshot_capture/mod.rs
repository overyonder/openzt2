use bevy::prelude::*;
use openzt2_zoo_state::locomotion::locomotion_types::CaffeineMode;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    staff_snapshot_section_encoding::validate_staff_snapshot_records,
    staff_snapshot_types::StaffSnapshotRecord,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::staff::staff_employment_types::Employment;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

pub(in crate::persistence) type StaffSnapshotEntities<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static PersistentId>,
        Option<&'static DefinitionId>,
        &'static Transform,
        &'static Employment,
        &'static StaffRole,
        Option<&'static CaffeineMode>,
    ),
>;

pub(super) fn capture_staff_snapshot_records(
    staff: &StaffSnapshotEntities,
) -> Result<Vec<StaffSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let mut records = staff
        .iter()
        .map(
            |(persistent_identifier, definition, transform, employment, role, caffeine)| {
                Ok(StaffSnapshotRecord {
                    persistent_identifier: persistent_identifier
                        .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?
                        .0,
                    definition: definition
                        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
                        .0,
                    role: role.0,
                    translation: transform.translation.to_array(),
                    rotation: transform.rotation.to_array(),
                    scale: transform.scale.to_array(),
                    wage_cents_per_month: employment.wage.0,
                    hired_tick: employment.hired_tick,
                    hired_month_ordinal: employment.hired_month_ordinal,
                    caffeinated: caffeine.map(|mode| *mode == CaffeineMode::Caffeinated),
                })
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    validate_staff_snapshot_records(&records)?;
    Ok(records)
}

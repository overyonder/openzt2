use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;

use super::maintenance_types::{
    LooseLitterSweepingStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
};

pub(super) fn classify_spawned_staff_jobs_by_maintenance_work_kind(
    mut commands: Commands,
    spawned_staff_jobs: Query<(Entity, &StaffJob), Added<StaffJob>>,
) {
    for (staff_job_entity, staff_job) in &spawned_staff_jobs {
        match staff_job.kind {
            StaffJobKind::EmptyBin => commands
                .entity(staff_job_entity)
                .insert(WasteContainerEmptyingStaffJobKindMarker),
            StaffJobKind::SweepLitter => commands
                .entity(staff_job_entity)
                .insert(LooseLitterSweepingStaffJobKindMarker),
            _ => continue,
        };
    }
}

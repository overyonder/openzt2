use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, WaterQuality};

use super::{
    staff_job_types::StaffJob,
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobRequest},
};

/// ponytail: `ZTAIStaffWaterCleaningMgr DirtThreshold="50"` and its keeper
/// request (`t_CleanTank`, priority 6) are identical in every
/// `config/*_mode.xml`; lower them when a mode authors different values.
const DIRT_THRESHOLD_PERMILLE: u16 = 500;
/// Request urgency is kept in half-priority units.
const URGENCY: u16 = 12;

fn clean_tank_token() -> AssetId {
    AssetId::from_key("t_cleantank")
}

/// A tank dirtier than the threshold asks a keeper to clean it; one cleaner
/// than that no longer needs its open request.
pub(crate) fn request_keeper_cleaning_for_dirty_tanks(
    tanks: Query<(Entity, &WaterQuality), (With<Tank>, Changed<WaterQuality>)>,
    jobs: Query<(Entity, &StaffJob)>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for (tank, quality) in &tanks {
        let dirt = 1_000_u16.saturating_sub(quality.0.min(1_000));
        if dirt > DIRT_THRESHOLD_PERMILLE {
            requests.write(StaffJobRequest {
                kind: StaffJobKind::MaintainTank,
                target: tank,
                urgency: URGENCY,
                token: Some(clean_tank_token()),
            });
            continue;
        }
        for (job, _) in jobs.iter().filter(|(_, job)| {
            job.kind == StaffJobKind::MaintainTank
                && job.target == tank
                && job.token == Some(clean_tank_token())
        }) {
            cancellations.write(CancelStaffJobRequest { job });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_tank_requests_cleaning_and_a_clean_one_cancels_it() {
        let mut app = App::new();
        app.add_message::<StaffJobRequest>()
            .add_message::<CancelStaffJobRequest>()
            .add_systems(Update, request_keeper_cleaning_for_dirty_tanks);
        let tank = app
            .world_mut()
            .spawn((
                Tank {
                    definition: AssetId::default(),
                },
                WaterQuality(400),
            ))
            .id();
        app.update();
        let requested = app
            .world_mut()
            .resource_mut::<Messages<StaffJobRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requested.len(), 1);
        assert_eq!(requested[0].kind, StaffJobKind::MaintainTank);
        let job = app
            .world_mut()
            .spawn(StaffJob {
                kind: StaffJobKind::MaintainTank,
                target: tank,
                urgency: URGENCY,
                token: Some(clean_tank_token()),
            })
            .id();
        app.world_mut().entity_mut(tank).insert(WaterQuality(900));
        app.update();
        let cancelled = app
            .world_mut()
            .resource_mut::<Messages<CancelStaffJobRequest>>()
            .drain()
            .map(|request| request.job)
            .collect::<Vec<_>>();
        assert_eq!(cancelled, [job]);
    }
}

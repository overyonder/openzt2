use bevy::{platform::collections::HashSet, prelude::*};

use super::{
    staff_employment_types::Staff,
    staff_job_types::{CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask},
    staff_lifecycle_messages::{FireStaffRequest, StaffFired},
};

pub(in crate::plugins::staff) fn release_claimed_job_and_despawn_fired_staff(
    mut commands: Commands,
    mut requests: MessageReader<FireStaffRequest>,
    staff: Query<Option<&CurrentJob>, With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim)>,
    mut fired: MessageWriter<StaffFired>,
    mut handled: Local<HashSet<Entity>>,
) {
    handled.clear();
    for request in requests.read() {
        if !handled.insert(request.staff) {
            continue;
        }
        let Ok(current) = staff.get(request.staff) else {
            continue;
        };
        let staff_entity = request.staff;
        if let Some(current) = current {
            if jobs
                .get(current.job)
                .is_ok_and(|(_, claim)| claim.staff == staff_entity)
            {
                commands
                    .entity(current.job)
                    .remove::<(JobClaim, StaffJobBehaviorTask)>();
                if let Ok((job, _)) = jobs.get(current.job) {
                    commands.entity(current.job).insert(*job);
                }
            }
        }
        fired.write(StaffFired {
            staff: staff_entity,
        });
        commands.entity(staff_entity).despawn();
    }
}

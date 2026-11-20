use bevy::{platform::collections::HashSet, prelude::*};

use super::{
    staff_employment_types::Staff,
    staff_lifecycle_messages::{FireStaffRequest, StaffFired},
};

pub(crate) fn release_claimed_job_and_despawn_fired_staff(
    mut commands: Commands,
    mut requests: MessageReader<FireStaffRequest>,
    staff: Query<(), With<Staff>>,
    mut fired: MessageWriter<StaffFired>,
    mut handled: Local<HashSet<Entity>>,
) {
    handled.clear();
    for request in requests.read() {
        if !handled.insert(request.staff) {
            continue;
        }
        if !staff.contains(request.staff) {
            continue;
        }
        let staff_entity = request.staff;
        fired.write(StaffFired {
            staff: staff_entity,
        });
        commands.entity(staff_entity).despawn();
    }
}

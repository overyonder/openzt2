use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::persistent_id_assignment::AssignPersistentId;
use openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssigned;
use openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssignmentFailed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdError;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

pub(super) fn assign_requested_persistent_ids(
    mut commands: Commands,
    mut requests: MessageReader<AssignPersistentId>,
    mut allocator: ResMut<PersistentIdAllocator>,
    members: Query<&WorldMember, Without<PersistentId>>,
    mut assigned: MessageWriter<PersistentIdAssigned>,
    mut failed: MessageWriter<PersistentIdAssignmentFailed>,
) {
    let mut assigned_this_update = HashSet::new();
    for request in requests.read() {
        let Ok(member) = members.get(request.entity) else {
            failed.write(PersistentIdAssignmentFailed {
                entity: request.entity,
                reason: PersistentIdError::WrongWorld,
            });
            continue;
        };
        if member.root != request.root || assigned_this_update.contains(&request.entity) {
            failed.write(PersistentIdAssignmentFailed {
                entity: request.entity,
                reason: PersistentIdError::WrongWorld,
            });
            continue;
        }
        match allocator.allocate(request.root) {
            Ok(id) => {
                assigned_this_update.insert(request.entity);
                commands.entity(request.entity).insert(id);
                assigned.write(PersistentIdAssigned {
                    entity: request.entity,
                    id,
                });
            }
            Err(reason) => {
                failed.write(PersistentIdAssignmentFailed {
                    entity: request.entity,
                    reason,
                });
            }
        }
    }
}

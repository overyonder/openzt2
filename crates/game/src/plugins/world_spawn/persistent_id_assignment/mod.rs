use bevy::{platform::collections::HashSet, prelude::*};

use super::{
    persistent_id_types::{PersistentId, PersistentIdAllocator, PersistentIdError},
    world_membership_types::WorldMember,
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssignPersistentId {
    pub(crate) entity: Entity,
    pub(crate) root: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PersistentIdAssigned {
    pub(super) entity: Entity,
    pub(super) id: PersistentId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PersistentIdAssignmentFailed {
    pub(super) entity: Entity,
    pub(super) reason: PersistentIdError,
}

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

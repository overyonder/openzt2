use bevy::prelude::*;

use super::{
    persistent_id_types::PersistentIdAllocator,
    world_membership_types::{WorldMember, WorldRoot},
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnloadWorld(pub(crate) Entity);

pub(super) fn unload_requested_worlds_and_their_members(
    mut commands: Commands,
    mut requests: MessageReader<UnloadWorld>,
    members: Query<(Entity, &WorldMember)>,
    roots: Query<(), With<WorldRoot>>,
    allocator: Option<Res<PersistentIdAllocator>>,
) {
    for request in requests.read() {
        if roots.get(request.0).is_err() {
            continue;
        }
        for (entity, member) in &members {
            if member.root == request.0 {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(request.0).despawn();
        if allocator
            .as_ref()
            .is_some_and(|value| value.owns_world(request.0))
        {
            commands.remove_resource::<PersistentIdAllocator>();
        }
    }
}

pub(super) fn despawn_all_world_roots_and_members_when_leaving_ingame(
    mut commands: Commands,
    members: Query<Entity, With<WorldMember>>,
    roots: Query<Entity, With<WorldRoot>>,
) {
    for entity in &members {
        commands.entity(entity).despawn();
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    commands.remove_resource::<PersistentIdAllocator>();
}

use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

pub(super) fn retire_world_root(commands: &mut Commands, root: Entity) {
    commands.entity(root).try_despawn();
}

pub fn unload_requested_worlds_and_their_members(
    mut commands: Commands,
    mut requests: MessageReader<UnloadWorld>,
    roots: Query<(), (With<WorldRoot>, Allow<Disabled>)>,
) {
    let mut seen = EntityHashSet::new();
    for root in requests.read().map(|request| request.0) {
        if roots.contains(root) && seen.insert(root) {
            retire_world_root(&mut commands, root);
        }
    }
}

pub fn despawn_all_world_roots_and_members_when_leaving_ingame(
    mut commands: Commands,
    roots: Query<Entity, (With<WorldRoot>, Allow<Disabled>)>,
) {
    for root in &roots {
        retire_world_root(&mut commands, root);
    }
}

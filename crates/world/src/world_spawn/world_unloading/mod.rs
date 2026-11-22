use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::prelude::*;
use openzt2_world_state::simulation_time::simulation_time_resource_removal::remove_world_simulation_time_resources;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldClockInitialized;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

pub(super) fn retire_world_root(commands: &mut Commands, root: Entity, initialized_clock: bool) {
    commands.entity(root).try_despawn();
    if initialized_clock {
        remove_world_simulation_time_resources(commands);
    }
}

pub fn unload_requested_worlds_and_their_members(
    mut commands: Commands,
    mut requests: MessageReader<UnloadWorld>,
    roots: Query<Has<WorldClockInitialized>, (With<WorldRoot>, Allow<Disabled>)>,
) {
    let mut seen = EntityHashSet::new();
    for root in requests.read().map(|request| request.0) {
        if let Ok(initialized_clock) = roots.get(root) {
            if !seen.insert(root) {
                continue;
            }
            retire_world_root(&mut commands, root, initialized_clock);
        }
    }
}

pub fn despawn_all_world_roots_and_members_when_leaving_ingame(
    mut commands: Commands,
    roots: Query<(Entity, Has<WorldClockInitialized>), (With<WorldRoot>, Allow<Disabled>)>,
) {
    for (root, initialized_clock) in &roots {
        retire_world_root(&mut commands, root, initialized_clock);
    }
}
use bevy::ecs::entity::EntityHashSet;

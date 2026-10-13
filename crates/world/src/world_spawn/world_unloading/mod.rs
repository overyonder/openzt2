use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::prelude::*;
use openzt2_world_state::simulation_time::simulation_time_resource_removal::remove_world_simulation_time_resources;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::queue_owned_hierarchy_roots_for_retirement;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

pub fn unload_requested_worlds_and_their_members(
    mut commands: Commands,
    mut requests: MessageReader<UnloadWorld>,
    members: Query<(Entity, &WorldMember), Allow<Disabled>>,
    roots: Query<(), (With<WorldRoot>, Allow<Disabled>)>,
    parents: Query<&ChildOf, Allow<Disabled>>,
    allocator: Option<Res<PersistentIdAllocator>>,
) {
    let requested_roots: EntityHashSet = requests.read().map(|request| request.0).collect();
    if requested_roots.is_empty() {
        return;
    }
    let live_roots: EntityHashSet = requested_roots
        .iter()
        .copied()
        .filter(|root| roots.contains(*root))
        .collect();
    let mut owned_entities = live_roots.clone();
    owned_entities.extend(
        members
            .iter()
            .filter_map(|(entity, member)| live_roots.contains(&member.root).then_some(entity)),
    );
    queue_owned_hierarchy_roots_for_retirement(&mut commands, &owned_entities, &parents);
    if allocator
        .as_ref()
        .is_some_and(|value| requested_roots.iter().any(|root| value.owns_world(*root)))
    {
        commands.remove_resource::<PersistentIdAllocator>();
        remove_world_simulation_time_resources(&mut commands);
    }
}

pub fn despawn_all_world_roots_and_members_when_leaving_ingame(
    mut commands: Commands,
    members: Query<Entity, (With<WorldMember>, Allow<Disabled>)>,
    roots: Query<Entity, (With<WorldRoot>, Allow<Disabled>)>,
    parents: Query<&ChildOf, Allow<Disabled>>,
) {
    let owned_entities = members.iter().chain(roots.iter()).collect();
    queue_owned_hierarchy_roots_for_retirement(&mut commands, &owned_entities, &parents);
    commands.remove_resource::<PersistentIdAllocator>();
}

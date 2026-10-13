use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, query::Allow},
    prelude::*,
};

use crate::plugins::simulation_time::simulation_time_world_lifecycle::remove_world_simulation_time_resources;

use super::{
    persistent_id_types::PersistentIdAllocator,
    world_membership_types::{WorldMember, WorldRoot},
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnloadWorld(pub(crate) Entity);

pub(super) fn unload_requested_worlds_and_their_members(
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

pub(in crate::plugins) fn despawn_all_world_roots_and_members_when_leaving_ingame(
    mut commands: Commands,
    members: Query<Entity, (With<WorldMember>, Allow<Disabled>)>,
    roots: Query<Entity, (With<WorldRoot>, Allow<Disabled>)>,
    parents: Query<&ChildOf, Allow<Disabled>>,
) {
    let owned_entities = members.iter().chain(roots.iter()).collect();
    queue_owned_hierarchy_roots_for_retirement(&mut commands, &owned_entities, &parents);
    commands.remove_resource::<PersistentIdAllocator>();
}

pub(in crate::plugins) fn queue_owned_hierarchy_roots_for_retirement(
    commands: &mut Commands,
    owned_entities: &EntityHashSet,
    parents: &Query<&ChildOf, Allow<Disabled>>,
) {
    // Unmarked renderer nodes can bridge two owned members. Only an ancestor
    // in this retirement operation owns their recursive despawn.
    for entity in owned_entities {
        if !parents
            .iter_ancestors::<ChildOf>(*entity)
            .any(|ancestor| owned_entities.contains(&ancestor))
        {
            commands.entity(*entity).despawn();
        }
    }
}

#[cfg(test)]
pub(in crate::plugins) fn assert_no_invalid_entity_commands_during(operation: impl FnOnce()) {
    use std::sync::{Arc, Mutex};

    struct WarningBuffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for WarningBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let warnings = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::clone(&warnings);
    let subscriber = bevy::log::tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(bevy::log::Level::WARN)
        .with_writer(move || WarningBuffer(Arc::clone(&writer)))
        .finish();
    bevy::log::tracing::subscriber::with_default(subscriber, operation);
    let warnings = String::from_utf8(warnings.lock().unwrap().clone()).unwrap();
    assert!(!warnings.contains("Entity despawned"), "{warnings}");
}

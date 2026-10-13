use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnloadWorld(pub Entity);

pub fn queue_owned_hierarchy_roots_for_retirement(
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

#[cfg(any(test, feature = "testing"))]
pub fn assert_no_invalid_entity_commands_during(operation: impl FnOnce()) {
    use std::sync::Arc;
    use std::sync::Mutex;

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

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldUnloadSet;

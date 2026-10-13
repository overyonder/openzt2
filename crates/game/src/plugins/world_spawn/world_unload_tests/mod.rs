use bevy::{ecs::entity_disabling::Disabled, prelude::*};
use openzt2_game_data::AssetId;

use super::{
    persistent_id_types::PersistentIdAllocator,
    world_membership_types::{WorldMember, WorldRoot},
    world_unloading::{
        assert_no_invalid_entity_commands_during, unload_requested_worlds_and_their_members,
        UnloadWorld,
    },
};

#[derive(Resource)]
struct HierarchyRetirementExpectation {
    retired: Vec<Entity>,
    retained: Vec<Entity>,
}

fn assert_retirement_is_visible_to_the_next_system(world: &mut World) {
    let expectation = world.resource::<HierarchyRetirementExpectation>();
    for entity in &expectation.retired {
        assert!(world.get_entity(*entity).is_err(), "retained {entity}");
    }
    for entity in &expectation.retained {
        assert!(world.get_entity(*entity).is_ok(), "retired {entity}");
    }
}

#[test]
fn repeated_unload_retires_owned_hierarchies_before_the_next_system() {
    use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

    let mut app = App::new();
    app.add_message::<UnloadWorld>()
        .add_systems(Update, unload_requested_worlds_and_their_members)
        .add_systems(
            Update,
            assert_retirement_is_visible_to_the_next_system
                .after(unload_requested_worlds_and_their_members),
        )
        .edit_schedule(Update, |schedule| {
            schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
    let root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::from_key("world"),
        })
        .id();
    let prefab = app.world_mut().spawn(WorldMember { root }).id();
    let child = app
        .world_mut()
        .spawn((WorldMember { root }, ChildOf(prefab)))
        .id();
    let renderer_bridge = app.world_mut().spawn(ChildOf(child)).id();
    let nested_disabled = app
        .world_mut()
        .spawn((WorldMember { root }, Disabled, ChildOf(renderer_bridge)))
        .id();
    let renderer = app.world_mut().spawn(ChildOf(nested_disabled)).id();
    let surviving_parent = app.world_mut().spawn_empty().id();
    let standalone = app
        .world_mut()
        .spawn((WorldMember { root }, ChildOf(surviving_parent)))
        .id();
    let disabled_root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: AssetId::from_key("disabled"),
            },
            Disabled,
        ))
        .id();
    let disabled_member = app
        .world_mut()
        .spawn((
            WorldMember {
                root: disabled_root,
            },
            Disabled,
        ))
        .id();
    let other_root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::from_key("other"),
        })
        .id();
    let other_member = app
        .world_mut()
        .spawn((
            WorldMember { root: other_root },
            ChildOf(other_root),
            Disabled,
        ))
        .id();
    let unrelated = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root))
        .insert_resource(ZooClock {
            tick: 17,
            absolute_day: 1,
            tick_in_day: 2,
        });
    app.insert_resource(HierarchyRetirementExpectation {
        retired: vec![
            root,
            prefab,
            child,
            renderer_bridge,
            nested_disabled,
            renderer,
            standalone,
            disabled_root,
            disabled_member,
        ],
        retained: vec![surviving_parent, other_root, other_member, unrelated],
    });
    for requested in [root, root, disabled_root, disabled_root] {
        app.world_mut().write_message(UnloadWorld(requested));
    }
    assert_no_invalid_entity_commands_during(|| app.update());
    assert!(!app.world().contains_resource::<PersistentIdAllocator>());
    assert!(!app.world().contains_resource::<ZooClock>());
    // A retained request cannot act on any later world or requeue old targets.
    app.world_mut().write_message(UnloadWorld(root));
    assert_no_invalid_entity_commands_during(|| app.update());
}

#[test]
fn delayed_unload_cannot_retire_a_later_worlds_allocator_or_simulation() {
    use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
    let mut app = App::new();
    app.add_message::<UnloadWorld>()
        .add_systems(Update, unload_requested_worlds_and_their_members);
    let old = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::from_key("old"),
        })
        .id();
    let later = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::from_key("later"),
        })
        .id();
    app.insert_resource(PersistentIdAllocator::new(later));
    let later_member = app
        .world_mut()
        .spawn((WorldMember { root: later }, Disabled))
        .id();
    let clock = ZooClock {
        tick: 17,
        absolute_day: 1,
        tick_in_day: 2,
    };
    app.insert_resource(clock);
    app.world_mut().write_message(UnloadWorld(old));
    app.update();
    assert!(app.world().get_entity(old).is_err());
    assert!(app.world().get_entity(later).is_ok());
    assert!(app.world().get_entity(later_member).is_ok());
    assert!(app
        .world()
        .resource::<PersistentIdAllocator>()
        .owns_world(later));
    assert_eq!(*app.world().resource::<ZooClock>(), clock);
    app.world_mut().write_message(UnloadWorld(old));
    app.update();
    assert!(app
        .world()
        .resource::<PersistentIdAllocator>()
        .owns_world(later));
    assert_eq!(*app.world().resource::<ZooClock>(), clock);
}

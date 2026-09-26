use super::*;
use crate::plugins::{
    animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy,
    behavior_task_execution_types::BehaviorTaskExecutionPhase,
    locomotion::locomotion_types::NavigationFailure,
};
use openzt2_game_data::{
    behavior::{
        action::docking::BehaviorDockAction,
        document::{BehaviorDocument, BehaviorSet},
    },
    AssetId,
};

#[test]
fn docking_events_respect_request_frame_and_failure_precedence() {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<InteractionContainerOccupancy>()
        .insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<Arrived>()
        .add_message::<NavigationFailed>()
        .add_systems(
            Update,
            (
                fail_behavior_docking_actions_after_navigation_failure,
                finish_behavior_docking_actions_after_matching_arrival,
            )
                .chain(),
        );
    let program = AssetId::from_key("dock-test");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "dock-test".into(),
                subjects: vec![],
                actions: openzt2_game_data::behavior::document::BehaviorTaskActionPhase::Supported(
                    vec![BehaviorAction::Dock(BehaviorDockAction {
                        subject_node_name: None,
                        target_node_name: None,
                        target_animation_clip_asset_key: None,
                        locomotion_speed: None,
                        redock: false,
                    })],
                ),
            }]),
        ));
    let target = app.world_mut().spawn_empty().id();
    let task = BehaviorTaskExecutionState {
        execution_id: 1,
        program,
        target: Some(target),
        document,
        declaration: 0,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 0,
        repetitions: 0,
        next_action_tick: 1,
        interaction_slot: None,
        stack: Default::default(),
    };
    let pending = PendingBehaviorDockingCompletion {
        target,
        origin: task.create_return_frame_after_action(0),
        stack_depth: 0,
        request_id: 2,
    };
    let actor = app.world_mut().spawn((task.clone(), pending.clone())).id();
    app.world_mut().write_message(NavigationFailed {
        entity: actor,
        request_id: 1,
        reason: NavigationFailure::NoRoute,
    });
    app.world_mut().write_message(Arrived {
        entity: actor,
        request_id: 1,
        target: Some(target),
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_none());

    // Two immediate arrivals must advance only once, even before deferred removal.
    for _ in 0..2 {
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 2,
            target: Some(target),
        });
    }
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        1
    );
    assert!(app
        .world()
        .get::<PendingBehaviorDockingCompletion>(actor)
        .is_none());

    app.world_mut()
        .entity_mut(actor)
        .insert((task.clone(), pending.clone()));
    app.world_mut()
        .get_mut::<BehaviorTaskExecutionState>(actor)
        .unwrap()
        .program = AssetId::from_key("replacement");
    app.world_mut().write_message(Arrived {
        entity: actor,
        request_id: 2,
        target: Some(target),
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );

    app.world_mut().entity_mut(actor).insert((task, pending));
    app.world_mut().write_message(NavigationFailed {
        entity: actor,
        request_id: 2,
        reason: NavigationFailure::NoRoute,
    });
    app.world_mut().write_message(Arrived {
        entity: actor,
        request_id: 2,
        target: Some(target),
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_some());
    assert!(app
        .world()
        .get::<PendingBehaviorDockingCompletion>(actor)
        .is_none());
}

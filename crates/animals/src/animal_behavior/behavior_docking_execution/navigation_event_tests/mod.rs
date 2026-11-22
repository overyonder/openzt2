use super::*;
use crate::animal_behavior::behavior_container_admission::add_test_container_member;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::{
    behavior::{
        action::docking::BehaviorDockAction,
        action_record::BehaviorAction,
        document::{BehaviorDocument, BehaviorSet},
    },
    AssetId,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;

#[test]
fn docking_events_respect_request_frame_and_failure_precedence() {
    let mut app = App::new();
    app.add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::apply_behavior_instruction_completion).init_resource::<Assets<BehaviorDocumentAsset>>()
        .add_plugins(move |app: &mut App| { app.world_mut().spawn(ZooClock {
            fractional_tick: Default::default(),
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        }); })
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
                        reservation: None,
                        radial_distance_m: None,
                        arrival_radius_m: None,
                        rotation_tolerance_radians: 0.0,
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
        object: None,
        interaction_slot: None,
        depth: 0,
    };
    let actor = app.world_mut().spawn(task.clone()).id();
    let bind_completion = |world: &mut World| {
        let instruction = world.get::<RunningBehaviorInstruction>(actor).unwrap().id();
        world
            .entity_mut(actor)
            .insert(PendingBehaviorDockingCompletion {
                instruction,
                target,
                request_id: 2,
            });
    };
    bind_completion(app.world_mut());
    add_test_container_member(app.world_mut(), target, actor, 0);
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
    assert!(app
        .world()
        .get::<DockedAtInteractionContainer>(actor)
        .is_some());
    add_test_container_member(app.world_mut(), target, actor, 1);
    assert!(app
        .world()
        .get::<DockedAtInteractionContainer>(actor)
        .is_none());

    app.world_mut().entity_mut(actor).insert(task.clone());
    bind_completion(app.world_mut());
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

    app.world_mut().entity_mut(actor).insert(task);
    bind_completion(app.world_mut());
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

use super::*;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_game_data::{
    behavior::{
        action::animation::BehaviorAnimationClipAction,
        document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    },
    AssetId,
};

#[test]
fn restarted_clip_ignores_old_outcomes_and_consumes_its_completion_once() {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<AnimationClipPlaybackRequest>()
        .add_message::<AnimationClipPlaybackRequestRejected>()
        .add_message::<AnimationCompleted>()
        .add_systems(
            Update,
            (
                start_current_behavior_animation_clip_actions,
                fail_behavior_animation_actions_after_playback_rejection,
                finish_behavior_animation_clip_actions_after_playback_completion,
            )
                .chain(),
        );
    let program = AssetId::from_key("clip-test");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "clip-test".into(),
                subjects: vec![],
                actions: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::AnimationClip(
                    BehaviorAnimationClipAction {
                        animation_clip_asset_key: "Stand_2StandItem".into(),
                        looping: false,
                    },
                )]),
            }]),
        ));
    let task = BehaviorTaskExecutionState {
        execution_id: 1,
        program,
        target: None,
        document,
        declaration: 0,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 0,
        repetitions: 0,
        next_action_tick: 0,
        interaction_slot: None,
        stack: Default::default(),
    };
    let actor = app.world_mut().spawn(task.clone()).id();
    let controller = app
        .world_mut()
        .spawn(AnimationPresentationOwner {
            gameplay_entity: actor,
        })
        .id();
    app.update();
    let old = app
        .world()
        .get::<PendingBehaviorAnimationClipCompletion>(actor)
        .unwrap()
        .request_id;
    app.world_mut()
        .entity_mut(actor)
        .remove::<PendingBehaviorAnimationClipCompletion>()
        .insert(task.clone());
    app.update();
    let current = app
        .world()
        .get::<PendingBehaviorAnimationClipCompletion>(actor)
        .unwrap()
        .request_id;
    assert_ne!(old, current);
    app.world_mut()
        .write_message(AnimationClipPlaybackRequestRejected {
            request_id: old,
            animation_subject_entity: actor,
            animation_clip_asset_key: "Stand_2StandItem".into(),
        });
    app.world_mut().write_message(AnimationCompleted {
        animation_playback_controller_entity: controller,
        playback_generation: 1,
        explicit_clip_request_id: Some(old),
    });
    app.update();
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_none());
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
    app.world_mut()
        .get_mut::<BehaviorTaskExecutionState>(actor)
        .unwrap()
        .target = Some(controller);
    app.world_mut().write_message(AnimationCompleted {
        animation_playback_controller_entity: controller,
        playback_generation: 2,
        explicit_clip_request_id: Some(current),
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
    app.world_mut()
        .get_mut::<BehaviorTaskExecutionState>(actor)
        .unwrap()
        .target = None;
    // Completion is valid even before next_action_tick, and after presentation
    // has moved on. The message owns the request identity, not the controller.
    for _ in 0..2 {
        app.world_mut().write_message(AnimationCompleted {
            animation_playback_controller_entity: controller,
            playback_generation: 2,
            explicit_clip_request_id: Some(current),
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
        .get::<PendingBehaviorAnimationClipCompletion>(actor)
        .is_none());
    app.world_mut().entity_mut(actor).insert(task);
    app.update();
    let refused = app
        .world()
        .get::<PendingBehaviorAnimationClipCompletion>(actor)
        .unwrap()
        .request_id;
    app.world_mut()
        .write_message(AnimationClipPlaybackRequestRejected {
            request_id: refused,
            animation_subject_entity: actor,
            animation_clip_asset_key: "Stand_2StandItem".into(),
        });
    app.world_mut().write_message(AnimationCompleted {
        animation_playback_controller_entity: controller,
        playback_generation: 3,
        explicit_clip_request_id: Some(refused),
    });
    app.update();
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_some());
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
}

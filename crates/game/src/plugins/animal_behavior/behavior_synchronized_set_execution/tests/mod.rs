use super::*;
use crate::plugins::{
    behavior_task_execution_types::{
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskFailed,
        BehaviorTaskFinished, PendingBehaviorTaskFailure,
    },
    simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_game_data::AssetId;

fn paired_tasks() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.insert_resource(ZooClock {
        tick: 20,
        absolute_day: 0,
        tick_in_day: 20,
    });
    app.add_message::<BehaviorTaskFinished>()
        .add_message::<BehaviorTaskFailed>();
    app.add_systems(Update, finish::finish_or_cancel_synchronized_behavior_sets);
    let target = app.world_mut().spawn_empty().id();
    let task = BehaviorTaskExecutionState {
        execution_id: 1,
        program: AssetId::from_key("purchase"),
        target: Some(target),
        document: Handle::default(),
        declaration: 0,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 3,
        repetitions: 0,
        next_action_tick: 0,
        interaction_slot: Some(0),
        stack: Default::default(),
    };
    let origin = task.create_return_frame_after_action(3);
    let actor = app
        .world_mut()
        .spawn((
            task,
            SynchronizedBehaviorOwner {
                execution_id: 1,
                target_execution_id: 2,
                target,
                origin,
                depth: 0,
                target_outcome: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(target)
        .insert(SynchronizedBehaviorParticipant {
            owner: actor,
            execution_id: 2,
        });
    (app, actor, target)
}

#[test]
fn subject_waits_for_partner_then_advances_once_and_releases_suppression() {
    let (mut app, actor, target) = paired_tasks();
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .expect("subject")
            .action,
        3
    );
    app.world_mut().write_message(BehaviorTaskFinished {
        actor: target,
        execution_id: 2,
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .expect("subject")
            .action,
        4
    );
    assert!(app
        .world()
        .get::<SynchronizedBehaviorParticipant>(target)
        .is_none());
    assert!(app
        .world()
        .get::<SynchronizedBehaviorOwner>(actor)
        .is_none());
    app.update();
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .expect("subject")
            .action,
        4
    );
}

#[test]
fn partner_failure_enters_subject_failure_without_advancing_purchase() {
    let (mut app, actor, target) = paired_tasks();
    app.world_mut().write_message(BehaviorTaskFailed {
        actor: target,
        execution_id: 2,
    });
    app.update();
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_some());
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .expect("subject")
            .action,
        3
    );
    assert!(app
        .world()
        .get::<SynchronizedBehaviorParticipant>(target)
        .is_none());
}

#[test]
fn destroyed_owner_releases_surviving_partner() {
    let (mut app, actor, target) = paired_tasks();
    app.world_mut().despawn(actor);
    app.update();
    assert!(app
        .world()
        .get::<SynchronizedBehaviorParticipant>(target)
        .is_none());
}

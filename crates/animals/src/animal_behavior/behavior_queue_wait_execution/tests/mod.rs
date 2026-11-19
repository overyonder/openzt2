use super::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;

#[test]
fn queue_deadline_keeps_its_parent_instruction_across_nested_waiting_sets() {
    let mut world = World::new();
    let target = world.spawn_empty().id();
    let mut task = BehaviorTaskExecutionState {
        execution_id: 1,
        program: AssetId::from_key("queue"),
        target: Some(target),
        document: Handle::default(),
        declaration: 0,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 2,
        repetitions: 0,
        next_action_tick: 1,
        object: None,
        interaction_slot: Some(0),
        depth: 0,
    };
    let wait = ActiveQueueWait {
        execution_id: task.execution_id,
        origin: task.create_return_frame_after_action(2),
        stack_depth: 0,
        target,
        queue_slot: 0,
        deadline_tick: 90,
    };
    let origin = wait.origin.clone();
    let actor = world.spawn((task.clone(), wait)).id();
    use bevy::ecs::system::RunSystemOnce;
    use openzt2_zoo_state::behavior_task_execution_types::{
        continuations::{
            BehaviorContinuationFrames, BehaviorContinuationOf, SuspendedBehaviorFrame,
        },
        running_actions::{
            resume_suspended_behavior_instruction, ResumeSuspendedBehaviorInstruction,
        },
    };
    world.add_observer(resume_suspended_behavior_instruction);
    let owns = move |frames: BehaviorContinuationFrames,
                     waits: Query<(&ActiveQueueWait, &BehaviorTaskExecutionState)>| {
        let (wait, task) = waits.get(actor).unwrap();
        wait.still_owns_task(actor, task, &frames)
    };
    assert!(world.run_system_once(owns).unwrap());
    world.spawn((
        BehaviorContinuationOf(actor),
        SuspendedBehaviorFrame(origin.clone()),
    ));
    task.depth = 1;
    task.program = AssetId::from_key("waiting");
    task.action = 4;
    *world.get_mut::<BehaviorTaskExecutionState>(actor).unwrap() = task;
    assert!(world.run_system_once(owns).unwrap());
    world.trigger(ResumeSuspendedBehaviorInstruction {
        actor,
        execution_id: 1,
        origin: origin.clone(),
        depth: 0,
        instruction: origin,
        tick: 20,
        advance: false,
    });
    world.flush();
    let task = world.get::<BehaviorTaskExecutionState>(actor).unwrap();
    assert_eq!(task.program, AssetId::from_key("queue"));
    assert_eq!(task.action, 2);
    assert_eq!(task.next_action_tick, 20);
    assert_eq!(task.depth, 0);
    assert_eq!(
        world.get::<ActiveQueueWait>(actor).unwrap().deadline_tick,
        90
    );
    world
        .get_mut::<BehaviorTaskExecutionState>(actor)
        .unwrap()
        .action = 3;
    assert!(!world.run_system_once(owns).unwrap());
}

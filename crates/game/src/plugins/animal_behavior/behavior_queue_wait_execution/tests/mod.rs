use super::*;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_game_data::AssetId;

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
        interaction_slot: Some(0),
        stack: Default::default(),
    };
    let wait = ActiveQueueWait {
        origin: task.create_return_frame_after_action(2),
        stack_depth: 0,
        target,
        queue_slot: 0,
        deadline_tick: 90,
    };
    assert!(wait.still_owns_task(&task));
    assert!(task.push_return_frame(wait.origin.clone()));
    task.program = AssetId::from_key("waiting");
    task.action = 4;
    assert!(wait.still_owns_task(&task));
    wait.restore_wait_instruction(&mut task, 20);
    assert_eq!(task.program, AssetId::from_key("queue"));
    assert_eq!(task.action, 2);
    assert_eq!(task.next_action_tick, 20);
    assert!(task.stack.is_empty());
    assert_eq!(wait.deadline_tick, 90);
    task.action = 3;
    assert!(!wait.still_owns_task(&task));
}

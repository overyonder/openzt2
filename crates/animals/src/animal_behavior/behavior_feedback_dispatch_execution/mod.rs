use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorFeedback, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackDispatched;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

pub(super) fn dispatch_current_authored_behavior_feedback_and_advance(
    mut commands: Commands,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<(
        Entity,
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorFeedback,
        ),
    )>,
    mut dispatched_feedback: MessageWriter<BehaviorFeedbackDispatched>,
) {
    for (actor, (behavior_task, instruction, running_action)) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if !instruction.is_current(behavior_task) {
            continue;
        }
        let feedback = &running_action.0;
        let dispatch = BehaviorFeedbackDispatched {
            actor,
            document: behavior_task.document.clone(),
            declaration: behavior_task.declaration,
            phase: behavior_task.phase,
            action: behavior_task.action,
        };
        debug!(
            actor = ?dispatch.actor,
            document = ?dispatch.document.id(),
            declaration = dispatch.declaration,
            phase = ?dispatch.phase,
            action = dispatch.action,
            entries = feedback.entries.len(),
            "dispatched authored behavior feedback"
        );
        dispatched_feedback.write(dispatch);
        instruction.complete(actor, simulation_clock.tick, &mut commands);
    }
}

//! Authored head-look steps.

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorHeadLook, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

/// A head look never waits, so the behavior continues at once. Turning the
/// head is presentation only and isn't drawn yet.
pub(super) fn continue_after_authored_head_look_steps(
    mut commands: Commands,
    clock: Res<ZooClock>,
    actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorHeadLook,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, task, instruction, _) in &actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        debug!(?actor, program = ?task.program, action = task.action, "head look step continues");
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

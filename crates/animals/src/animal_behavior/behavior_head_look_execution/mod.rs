//! Authored head-look steps.

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorHeadLook, RunningBehaviorInstruction,
};

use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationHeadLook;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

/// Submit or release the look target and continue the behavior immediately.
pub(super) fn continue_after_authored_head_look_steps(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorHeadLook,
            Option<&mut AnimationHeadLook>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, task, instruction, action, current) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        if action.0.disengage {
            if current.is_some() {
                commands.entity(actor).try_remove::<AnimationHeadLook>();
            }
        } else if let Some(target) = task.target() {
            let wanted = AnimationHeadLook {
                target,
                target_node: action.0.target_node.clone(),
            };
            if let Some(mut current) = current {
                current.set_if_neq(wanted);
            } else {
                commands.entity(actor).try_insert(wanted);
            }
        }
        debug!(?actor, program = ?task.program, action = task.action, "head look step continues");
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

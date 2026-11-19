use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorLearnTrick, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::shows::performer_training::PerformerTraining;

pub(super) fn apply_current_authored_trick_learning(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut performers: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorLearnTrick,
            ),
            Option<&mut PerformerTraining>,
        ),
        (With<Animal>, Without<PendingBehaviorTaskFailure>),
    >,
) {
    for (performer, (task, instruction, running_action), training) in &mut performers {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let RunBehaviorLearnTrick { trick, amount, cap } = running_action;
        if let Some(mut training) = training {
            training.apply_capped_learning(*trick, *amount, *cap);
        } else {
            let mut training = PerformerTraining::default();
            training.apply_capped_learning(*trick, *amount, *cap);
            commands.entity(performer).insert(training);
        }
        instruction.complete(performer, clock.tick, &mut commands);
    }
}

use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::plugins::{
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    shows::performer_training::PerformerTraining,
    simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

pub(super) fn apply_current_authored_trick_learning(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut performers: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&mut PerformerTraining>,
        ),
        (With<Animal>, Without<PendingBehaviorTaskFailure>),
    >,
) {
    for (performer, mut task, training) in &mut performers {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::LearnTrick { trick, amount, cap }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        if let Some(mut training) = training {
            training.apply_capped_learning(*trick, *amount, *cap);
        } else {
            let mut training = PerformerTraining::default();
            training.apply_capped_learning(*trick, *amount, *cap);
            commands.entity(performer).insert(training);
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

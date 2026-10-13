//! Authored head-look steps.

use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::plugins::{
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

/// A head look never waits, so the behavior continues at once. Turning the
/// head is presentation only and isn't drawn yet.
pub(super) fn continue_after_authored_head_look_steps(
    documents: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut actors: Query<
        (Entity, &mut BehaviorTaskExecutionState),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, mut task) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !matches!(
            documents
                .get(&task.document)
                .and_then(|document| find_current_behavior_task_action(document, &task)),
            Some(BehaviorAction::HeadLook(_))
        ) {
            continue;
        }
        debug!(?actor, program = ?task.program, action = task.action, "head look step continues");
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

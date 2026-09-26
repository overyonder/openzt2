use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorFeedbackDispatched, BehaviorTaskExecutionState,
        },
        simulation_time::simulation_clock_types::ZooClock,
    },
};

pub(super) fn dispatch_current_authored_behavior_feedback_and_advance(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<(Entity, &mut BehaviorTaskExecutionState)>,
    mut dispatched_feedback: MessageWriter<BehaviorFeedbackDispatched>,
) {
    for (actor, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Feedback(feedback)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
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
        advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
    }
}

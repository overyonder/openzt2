//! Emits authored view events. Recipient delivery is not implemented yet.

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::plugins::behavior_task_execution_types::advance_behavior_task_to_next_action;
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::behavior_task_execution_types::BehaviorTaskReturnFrame;
use crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use bevy::prelude::*;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

/// References immutable authored data rather than copying feedback or view rows.
#[derive(Message)]
#[expect(
    dead_code,
    reason = "retain the typed authored view-event payload until recipient delivery is implemented"
)]
pub(crate) struct BehaviorViewEventEmitted {
    pub(crate) source: Entity,
    pub(crate) source_definition: AssetId,
    pub(crate) view_key: AssetId,
    pub(crate) action: BehaviorTaskReturnFrame,
}

pub(super) fn emit_authored_view_events(
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    mut tasks: Query<
        (
            Entity,
            Option<&DefinitionId>,
            &mut BehaviorTaskExecutionState,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut events: MessageWriter<BehaviorViewEventEmitted>,
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (source, definition, mut task) in &mut tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::ViewEvent(event)) = assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(definition) = definition.and_then(|id| definitions.find_object(id.0)) else {
            mark_behavior_task_for_failure_and_stop_navigation(source, &mut commands);
            continue;
        };
        if !definition
            .view_data
            .iter()
            .any(|row| row.name == event.view_key)
        {
            mark_behavior_task_for_failure_and_stop_navigation(source, &mut commands);
            continue;
        }
        events.write(BehaviorViewEventEmitted {
            source,
            source_definition: definition.id,
            view_key: event.view_key,
            action: task.create_return_frame_after_action(task.action),
        });
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

//! Timed queue waits interrupt their waiting set when service becomes available.

mod start;
#[cfg(test)]
mod tests;
mod update;

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
};
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(SystemParam)]
struct QueueWaitSources<'w, 's> {
    behaviors: Res<'w, Assets<BehaviorDocumentAsset>>,
    declarations: Res<'w, LoadedBehaviorDocumentCollection>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    targets: Query<'w, 's, &'static DefinitionId>,
}

/// The wait's suspended instruction and deadline; container membership is owned
/// separately by InteractionContainerOccupancy and is never copied here.
#[derive(Component)]
struct ActiveQueueWait {
    origin: BehaviorTaskReturnFrame,
    stack_depth: usize,
    target: Entity,
    queue_slot: usize,
    deadline_tick: u64,
}

impl ActiveQueueWait {
    fn still_owns_task(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.target() == Some(self.target)
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }

    fn restore_wait_instruction(&self, task: &mut BehaviorTaskExecutionState, tick: u64) {
        task.restore_suspended_instruction(&self.origin, self.stack_depth, tick);
    }
}

pub(super) fn register_queue_wait_execution(application: &mut App) {
    use crate::{application_lifecycle::GamePhase, application_schedule::FixedGameSet};
    application.add_systems(FixedUpdate, (
        update::update_queue_wait_deadlines_and_service_readiness,
        start::start_or_repeat_authored_queue_wait,
    ).chain()
        .before(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .in_set(FixedGameSet::Think)
        .run_if(in_state(GamePhase::InGame)));
}

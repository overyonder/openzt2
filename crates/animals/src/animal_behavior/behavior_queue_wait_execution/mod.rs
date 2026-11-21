//! Timed queue waits interrupt their waiting set when service becomes available.

mod start;
#[cfg(test)]
mod tests;
mod update;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskReturnFrame;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
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

/// The wait's suspended instruction and deadline for the requested queue slot.
#[derive(Component)]
struct ActiveQueueWait {
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    stack_depth: usize,
    target: Entity,
    queue_slot: usize,
    deadline_tick: u64,
}

impl ActiveQueueWait {
    fn still_owns_task(
        &self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        frames: &openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames,
    ) -> bool {
        self.execution_id == task.execution_id
            && task.target() == Some(self.target)
            && frames.owns(actor, task, &self.origin, self.stack_depth)
    }
}

pub(super) fn register_queue_wait_execution(application: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        BehaviorStep, BehaviorStepSet,
    };

    application
        .add_observer(start::apply_queue_wait_admission)
        .add_systems(
            BehaviorStep,
            (
                update::update_queue_wait_deadlines_and_service_readiness,
                start::start_or_repeat_authored_queue_wait,
            )
                .chain()
                .in_set(BehaviorStepSet::Actions)
                .run_if(in_state(GamePhase::InGame)),
        );
}

//! Coupled behavior execution retains one task per participating entity.

mod finish;
mod start;
#[cfg(test)]
mod tests;

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::behavior_task_execution_types::BehaviorTaskReturnFrame;
use crate::plugins::guests::guest_simulation_types::GuestArchetype;
use crate::plugins::staff::staff_employment_types::StaffRole;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use bevy::{ecs::system::SystemParam, prelude::*};

#[derive(Component)]
struct SynchronizedBehaviorOwner {
    execution_id: u64,
    target_execution_id: u64,
    target: Entity,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    target_outcome: Option<bool>,
}

/// While coupled, ordinary set requests must not replace the target's task.
#[derive(Component)]
pub(crate) struct SynchronizedBehaviorParticipant {
    execution_id: u64,
    owner: Entity,
}

#[derive(SystemParam)]
struct SynchronizedBehaviorSources<'w, 's> {
    documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    declarations: Res<'w, LoadedBehaviorDocumentCollection>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    identities: Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static StaffRole>,
            Option<&'static GuestArchetype>,
            Option<&'static DefinitionId>,
        ),
    >,
}

pub(super) fn register_synchronized_behavior_execution(app: &mut App) {
    use crate::{application_lifecycle::GamePhase, application_schedule::FixedGameSet};
    app.add_systems(FixedUpdate, (finish::finish_or_cancel_synchronized_behavior_sets, start::start_synchronized_behavior_sets)
        .chain().before(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .in_set(FixedGameSet::Think).run_if(in_state(GamePhase::InGame)));
}

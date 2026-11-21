//! Coupled behavior execution retains one task per participating entity.

mod finish;
mod start;
#[cfg(test)]
mod tests;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_zoo_state::animal_behavior::behavior_synchronized_set_types::SynchronizedBehaviorOwner;

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
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        BehaviorStep, BehaviorStepSet,
    };

    app.add_observer(start::claim_synchronized_behavior)
        .add_systems(
            BehaviorStep,
            (
                finish::finish_or_cancel_synchronized_behavior_sets,
                start::start_synchronized_behavior_sets,
            )
                .chain()
                .in_set(BehaviorStepSet::Actions)
                .run_if(in_state(GamePhase::InGame)),
        );
}

use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;

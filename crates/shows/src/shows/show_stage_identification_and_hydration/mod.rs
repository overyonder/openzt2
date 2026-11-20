use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::donations::donation_opportunity_types::DirectDonationSettlement;

use super::show_stage_types::{ShowStage, ShowStageHydrated, ShowStageOpenState};

pub(super) fn show_stage_definitions_are_available(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
) -> bool {
    active_definitions.get(&definitions).is_some()
}

pub(super) fn hydrate_show_stages_from_active_world_definitions(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut stages: Query<
        (
            Entity,
            &DefinitionId,
            Option<&mut ShowStage>,
            Has<ShowStageOpenState>,
        ),
        Without<ShowStageHydrated>,
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, definition_id, existing_stage, has_open_state) in &mut stages {
        let definition = existing_stage
            .as_deref()
            .and_then(|stage| definitions.find_show_stage(stage.show_stage_definition))
            .or_else(|| definitions.find_show_stage_definition_for_world_object(definition_id.0));
        let Some(definition) = definition else {
            continue;
        };
        let stage = ShowStage {
            show_stage_definition: definition.id,
        };
        if let Some(mut existing_stage) = existing_stage {
            existing_stage.set_if_neq(stage);
        } else {
            commands.entity(entity).insert(stage);
        }
        commands
            .entity(entity)
            .insert((ShowStageHydrated, DirectDonationSettlement));
        if !has_open_state {
            commands.entity(entity).insert(ShowStageOpenState::Open);
        }
    }
}

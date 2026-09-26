use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::donations::donation_opportunity_types::DirectDonationSettlement;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::show_stage_types::{ShowStage, ShowStageHydrated, ShowStageOpenState};

pub(crate) fn identify_and_hydrate_spawned_show_stage_world_objects(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    stages: Query<
        (
            Entity,
            &DefinitionId,
            Option<&ShowStage>,
            Has<ShowStageOpenState>,
        ),
        (
            Without<ShowStageHydrated>,
            Or<(Added<DefinitionId>, Added<ShowStage>)>,
        ),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, world_object_definition, existing_show_stage, has_open_state) in &stages {
        let show_stage_definition = existing_show_stage
            .and_then(|show_stage| definitions.find_show_stage(show_stage.show_stage_definition))
            .or_else(|| {
                definitions.find_show_stage_definition_for_world_object(world_object_definition.0)
            });
        let Some(show_stage_definition) = show_stage_definition else {
            continue;
        };
        let mut stage = commands.entity(entity);
        stage.insert((
            DirectDonationSettlement,
            ShowStage {
                show_stage_definition: show_stage_definition.id,
            },
            ShowStageHydrated,
        ));
        if !has_open_state {
            stage.insert(ShowStageOpenState::Open);
        }
    }
}

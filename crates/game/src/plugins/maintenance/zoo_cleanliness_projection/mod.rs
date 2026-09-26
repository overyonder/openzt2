use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    cleanliness_aggregation_operations::calculate_weighted_zoo_cleanliness_permille,
    maintenance_types::{IncrementalZooCleanlinessTotals, ZooCleanlinessPermille},
};

pub(crate) fn project_changed_maintenance_totals_into_zoo_cleanliness(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
    mut zoo_cleanliness: ResMut<ZooCleanlinessPermille>,
) {
    if !cleanliness_totals.requires_recalculation
        && !active_world_definitions.is_changed()
        && !world_definition_assets.is_changed()
    {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        zoo_cleanliness.set_if_neq(ZooCleanlinessPermille(None));
        return;
    };
    let projected = world_definitions
        .cleanliness_policy()
        .map(|policy| calculate_weighted_zoo_cleanliness_permille(*cleanliness_totals, policy));
    zoo_cleanliness.set_if_neq(ZooCleanlinessPermille(projected));
    cleanliness_totals.requires_recalculation = false;
}

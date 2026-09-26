use bevy::prelude::{
    Assets, Commands, Entity, Name, Query, Res, ResMut, Transform, Vec3, Visibility,
};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::topology::topology_graph_types::PathTile;
use crate::plugins::topology::topology_graph_types::TopologyGrid;

use super::{
    persistent_id_types::PersistentId,
    world_hydration_types::WorldHydration,
    world_load_failure::WorldLoadFailure,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::WorldMember,
};

/// Place the starting paths on the terrain grid.
pub(super) fn hydrate_all_starting_paths_into_topology_once(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    mut pending: Query<(Entity, &mut WorldHydration)>,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::StartingPathHydration);
    let Ok((root, mut pending)) = pending.single_mut() else {
        return;
    };
    if !pending.starting_path_hydration_is_pending() {
        return;
    }
    let Some(asset) = scenarios.get(pending.starting_zoo_document_asset_handle()) else {
        pending.record_failure(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(start) = asset.document.find_starting_zoo(pending.starting_zoo_id()) else {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let paths = start.paths.as_slice();
    for path in paths {
        let definition_id = AssetId(path.definition.0);
        if definitions.find_path(definition_id).is_none() {
            if !active_definitions.is_complete() {
                return;
            }
            pending.record_failure(WorldLoadFailure::MissingDependency(definition_id));
            return;
        }
    }

    for path in paths {
        let definition_id = AssetId(path.definition.0);
        let position = Vec3::new(path.position_m[0], path.position_m[1], path.position_m[2]);
        let mut cell = grid.position_cell(position);
        let elevated = definitions
            .find_path(definition_id)
            .map(|definition| definition.elevated)
            .expect("validated authored starting-path definition");
        if !elevated {
            cell.z = 0;
        }
        commands.spawn((
            Name::new("starting path"),
            PathTile {
                definition: definition_id,
                cell,
            },
            PersistentId(path.persistent_id),
            Transform::from_translation(position),
            Visibility::Inherited,
            WorldMember { root },
        ));
    }
    pending.record_starting_path_hydration_completion();
}

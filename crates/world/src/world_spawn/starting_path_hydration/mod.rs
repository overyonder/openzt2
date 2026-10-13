use bevy::prelude::Assets;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Name;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::ResMut;
use bevy::prelude::Transform;
use bevy::prelude::Vec3;
use bevy::prelude::Visibility;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceStage;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

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

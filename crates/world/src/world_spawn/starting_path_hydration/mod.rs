use bevy::prelude::Assets;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Name;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::Transform;
use bevy::prelude::Vec3;
use bevy::prelude::Visibility;
use bevy::prelude::With;
use bevy::prelude::Without;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_load_completion_marker::{
    ImportedPersistentIdsReserved, StartingPathsHydrated, StartingTerrainHydrated,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// Place the starting paths on the terrain grid.
pub(super) fn hydrate_all_starting_paths_into_topology_once(
    mut commands: Commands,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    pending: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
            &TopologyGrid,
        ),
        (
            With<WorldHydration>,
            With<ImportedPersistentIdsReserved>,
            With<StartingTerrainHydrated>,
            Without<StartingPathsHydrated>,
            Without<WorldLoadFailure>,
        ),
    >,
) {
    let Ok((root, selected_world, documents, grid)) = pending.single() else {
        return;
    };
    let Some(asset) = scenarios.get(&documents.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(start) = asset.document.find_starting_zoo(selected_world.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let paths = start.paths.as_slice();
    for path in paths {
        let definition_id = AssetId(path.definition.0);
        if definitions.find_path(definition_id).is_none() {
            if !active_definitions.is_complete() {
                return;
            }
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::MissingDependency(definition_id));
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
    commands.entity(root).insert(StartingPathsHydrated);
}

use bevy::prelude::{
    Assets, Commands, Entity, Name, Query, Res, ResMut, Transform, Vec3, Visibility,
};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::topology::gate_operation_types::Gate;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::topology::topology_graph_types::TopologyGrid;
use crate::plugins::topology::topology_graph_types::TopologyNode;
use crate::plugins::topology::topology_graph_types::TopologyProtected;
use crate::plugins::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

use super::{
    persistent_id_types::PersistentId,
    world_hydration_types::WorldHydration,
    world_load_failure::WorldLoadFailure,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::WorldMember,
};

/// Installs source-resolved starting fence endpoints and edges into the same
/// ECS topology used by construction. Legacy fence entities and their
/// position/rotation encoding do not reach the runtime.
pub(super) fn hydrate_all_starting_fence_nodes_and_edges_into_topology_once(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    mut pending: Query<(Entity, &mut WorldHydration)>,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::StartingFenceHydration);
    let Ok((root, mut pending)) = pending.single_mut() else {
        return;
    };
    if !pending.starting_topology_hydration_is_pending() {
        return;
    }
    let Some(asset) = scenarios.get(pending.starting_zoo_document_asset_handle()) else {
        pending.record_failure(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = &asset.document;
    let Some(start) = catalog.find_starting_zoo(pending.starting_zoo_id()) else {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let nodes = start.topology_nodes.as_slice();
    let fences = start.fences.as_slice();

    for fence in fences {
        let source_definition = AssetId(fence.definition.0);
        if resolve_starting_fence_definition(definitions, source_definition).is_none() {
            if !active_definitions.is_complete() {
                return;
            }
            pending.record_failure(WorldLoadFailure::MissingDependency(source_definition));
            return;
        }
        let a_index = fence.a as usize;
        let b_index = fence.b as usize;
        if nodes.get(a_index).is_none() || nodes.get(b_index).is_none() {
            pending.record_failure(WorldLoadFailure::InvalidRecord(
                u32::try_from(a_index.max(b_index)).unwrap_or(u32::MAX),
            ));
            return;
        }
    }

    let node_entities = nodes
        .iter()
        .map(|node| {
            let position = Vec3::new(node.position_m[0], node.position_m[1], node.position_m[2]);
            let mut cell = grid.position_cell(position);
            // Starting fences are source ground topology. Authored Y records
            // the terrain sample beneath the segment; it is not an elevated
            // topology layer. Terrain fitting owns the rendered height.
            cell.z = 0;
            commands
                .spawn((
                    Name::new("starting topology node"),
                    TopologyNode { cell },
                    PersistentId(node.persistent_id),
                    Transform::from_translation(grid.cell_translation(cell)),
                    WorldMember { root },
                ))
                .id()
        })
        .collect::<Vec<_>>();

    for fence in fences {
        let source_definition = AssetId(fence.definition.0);
        let (definition, gate) = resolve_starting_fence_definition(definitions, source_definition)
            .expect("validated authored starting-fence definition");
        let a_index = fence.a as usize;
        let b_index = fence.b as usize;
        let a = node_entities[a_index];
        let b = node_entities[b_index];
        let a_record = &nodes[a_index];
        let b_record = &nodes[b_index];
        let mut a_cell = grid.position_cell(Vec3::new(
            a_record.position_m[0],
            a_record.position_m[1],
            a_record.position_m[2],
        ));
        let mut b_cell = grid.position_cell(Vec3::new(
            b_record.position_m[0],
            b_record.position_m[1],
            b_record.position_m[2],
        ));
        a_cell.z = 0;
        b_cell.z = 0;
        let entity = commands
            .spawn((
                Name::new("starting fence"),
                FenceEdge { definition, a, b },
                PersistentId(fence.persistent_id),
                calculate_topology_edge_world_transform(a_cell, b_cell, *grid),
                Visibility::Inherited,
                WorldMember { root },
            ))
            .id();
        if fence.protected {
            commands.entity(entity).insert(TopologyProtected);
        }
        if gate || fence.gate {
            commands.entity(entity).insert(Gate {
                open: false,
                locked: false,
            });
        }
    }
    pending.record_starting_topology_hydration_completion();
}

fn resolve_starting_fence_definition(
    definitions: WorldDefinitionsView<'_>,
    source_definition: AssetId,
) -> Option<(AssetId, bool)> {
    definitions
        .find_fence(source_definition)
        .map(|record| (record.id, false))
        .or_else(|| {
            definitions
                .fences()
                .find(|record| record.gate == source_definition)
                .map(|record| (record.id, true))
        })
}

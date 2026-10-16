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
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_graph_types::TopologyProtected;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceStage;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

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

    // Saved curbs are path presentation. Importing them as fences makes them
    // compete with actual enclosure edges in the topology index.
    let curb_type = AssetId::from_key("curb");
    let fences = fences
        .iter()
        .filter(|fence| {
            resolve_starting_fence_definition(definitions, fence.definition)
                .and_then(|(id, _)| definitions.find_fence(id))
                .and_then(|fence| definitions.find_object(fence.object))
                .is_none_or(|object| !object.behavior_type_identifiers.contains(&curb_type))
        })
        .collect::<Vec<_>>();
    let used_nodes = fences
        .iter()
        .flat_map(|fence| [fence.a as usize, fence.b as usize])
        .collect::<std::collections::BTreeSet<_>>();
    let node_entities = used_nodes
        .into_iter()
        .map(|index| {
            let node = &nodes[index];
            let position = Vec3::new(node.position_m[0], node.position_m[1], node.position_m[2]);
            let mut cell = grid.position_cell(position);
            // Starting fences are source ground topology. Authored Y records
            // the terrain sample beneath the segment; it is not an elevated
            // topology layer. Terrain fitting owns the rendered height.
            cell.z = 0;
            let entity = commands
                .spawn((
                    Name::new("starting topology node"),
                    TopologyNode { cell },
                    PersistentId(node.persistent_id),
                    Transform::from_translation(grid.cell_translation(cell)),
                    WorldMember { root },
                ))
                .id();
            (index, entity)
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    for fence in fences {
        let source_definition = AssetId(fence.definition.0);
        let (definition, gate) = resolve_starting_fence_definition(definitions, source_definition)
            .expect("validated authored starting-fence definition");
        let a_index = fence.a as usize;
        let b_index = fence.b as usize;
        let a = node_entities[&a_index];
        let b = node_entities[&b_index];
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

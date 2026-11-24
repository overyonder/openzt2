use std::collections::{BTreeMap, BTreeSet};

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
    ImportedPersistentIdsReserved, StartingFenceTopologyHydrated, StartingTerrainHydrated,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
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
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// Installs source-resolved starting fence endpoints and edges into the same
/// ECS topology used by construction. Legacy fence entities and their
/// position/rotation encoding do not reach the runtime.
pub(super) fn hydrate_all_starting_fence_nodes_and_edges_into_topology_once(
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
            Without<StartingFenceTopologyHydrated>,
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
    let catalog = &asset.document;
    let Some(start) = catalog.find_starting_zoo(selected_world.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let nodes = start.topology_nodes.as_slice();
    let curb_type = AssetId::from_key("curb");
    let mut fences = Vec::with_capacity(start.fences.len());
    let mut fence_definitions_by_gate = None;
    for fence in &start.fences {
        let source_definition = fence.definition;
        let resolved = definitions
            .find_fence(source_definition)
            .map(|definition| (definition, false))
            .or_else(|| {
                fence_definitions_by_gate
                    .get_or_insert_with(|| {
                        let mut by_gate = BTreeMap::new();
                        for definition in definitions.fences() {
                            by_gate.entry(definition.gate).or_insert(definition);
                        }
                        by_gate
                    })
                    .get(&source_definition)
                    .copied()
                    .map(|definition| (definition, true))
            });
        let Some((definition, gate)) = resolved else {
            if active_definitions.is_complete() {
                commands
                    .entity(root)
                    .insert_if_new(WorldLoadFailure::MissingDependency(source_definition));
            }
            return;
        };
        let a_index = fence.a as usize;
        let b_index = fence.b as usize;
        if a_index >= nodes.len() || b_index >= nodes.len() {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::InvalidRecord(
                    u32::try_from(a_index.max(b_index)).unwrap_or(u32::MAX),
                ));
            return;
        }
        // Curbs belong to path presentation and must not occupy enclosure edges.
        if definitions
            .find_object(definition.object)
            .is_some_and(|object| object.behavior_type_identifiers.contains(&curb_type))
        {
            continue;
        }
        fences.push((fence, definition.id, gate));
    }
    let used_nodes = fences
        .iter()
        .flat_map(|(fence, ..)| [fence.a as usize, fence.b as usize])
        .collect::<BTreeSet<_>>();
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
            (index, (entity, cell))
        })
        .collect::<BTreeMap<_, _>>();

    for (fence, definition, gate) in fences {
        let (a, a_cell) = node_entities[&(fence.a as usize)];
        let (b, b_cell) = node_entities[&(fence.b as usize)];
        let entity = commands
            .spawn((
                Name::new("starting fence"),
                FenceEdge::between(definition, a, b),
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
    commands.entity(root).insert(StartingFenceTopologyHydrated);
}

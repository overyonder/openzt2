use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::gate_operation_types::GateAutoClose;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TileSurface;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

/// Restores transforms and definition properties for saved topology, retrying
/// while definitions are loading.
pub(super) fn hydrate_save_loaded_topology_definition_facts(
    mut commands: Commands,
    grid: Res<TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    nodes: Query<&TopologyNode>,
    fences: Query<(
        Entity,
        &FenceEdge,
        Option<&Gate>,
        Option<&GateMechanism>,
        Option<&GateAutoClose>,
        Option<&Transform>,
        Option<&FenceTerrainEndpointPresentationCells>,
        Option<&Visibility>,
    )>,
    paths: Query<(Entity, &PathTile, Option<&TileSurface>, Option<&Transform>)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, edge, gate, mechanism, timer, transform, endpoint_cells, visibility) in &fences {
        let (Ok(a), Ok(b)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
            continue;
        };
        if definitions.find_fence(edge.definition).is_none() {
            continue;
        }
        let mut entity_commands = commands.entity(entity);
        if transform.is_none() {
            entity_commands.insert(calculate_topology_edge_world_transform(
                a.cell, b.cell, *grid,
            ));
        }
        if endpoint_cells.is_none() {
            entity_commands.insert(FenceTerrainEndpointPresentationCells {
                first: a.cell,
                second: b.cell,
            });
        }
        if visibility.is_none() {
            entity_commands.insert(Visibility::Inherited);
        }
        if gate.is_some() && mechanism.is_none() {
            entity_commands.insert(GateMechanism::default());
        }
        if gate.is_some() && timer.is_none() {
            entity_commands.insert(GateAutoClose(0));
        }
    }

    for (entity, path, surface, transform) in &paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        let mut entity_commands = commands.entity(entity);
        if transform.is_none() {
            entity_commands.insert(Transform::from_translation(
                grid.cell_translation(path.cell),
            ));
        }
        if surface.is_none() {
            entity_commands.insert(TileSurface {
                height_cm: definition.surface.height_cm,
            });
        }
    }
}

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    gate_operation_types::{Gate, GateAutoClose, GateMechanism},
    topology_graph_types::{FenceEdge, PathTile, TileSurface, TopologyGrid, TopologyNode},
    topology_grid_geometry::calculate_topology_edge_world_transform,
    topology_presentation_types::FenceTerrainEndpointPresentationCells,
};

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
        let Some(definition) = definitions.find_fence(edge.definition) else {
            continue;
        };
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
            let policy = &definition.gate_policy;
            entity_commands.insert(GateMechanism {
                prefab: AssetId(policy.prefab.0),
                open_animation: AssetId(policy.open_animation.0),
                close_animation: AssetId(policy.close_animation.0),
                trigger_distance_cm: policy.trigger_distance_cm,
                auto_close_ticks: policy.auto_close_ticks,
            });
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

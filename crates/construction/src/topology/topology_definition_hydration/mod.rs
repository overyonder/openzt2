use bevy::prelude::*;
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TileSurface;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

/// Restores transforms and definition properties for saved topology, retrying
/// while definitions are loading.
pub(super) fn hydrate_save_loaded_topology_definition_facts(
    mut commands: Commands,
    grid: Single<&TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    nodes: Query<&TopologyNode>,
    fences: Query<
        (
            Entity,
            FenceSegment,
            Has<Gate>,
            Has<GateMechanism>,
            Has<Transform>,
            Has<FenceTerrainEndpointPresentationCells>,
            Has<Visibility>,
        ),
        Or<(
            Without<Transform>,
            Without<FenceTerrainEndpointPresentationCells>,
            Without<Visibility>,
            (With<Gate>, Without<GateMechanism>),
        )>,
    >,
    paths: Query<
        (Entity, &PathTile, Has<TileSurface>, Has<Transform>),
        Or<(Without<TileSurface>, Without<Transform>)>,
    >,
) {
    let grid = grid.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, edge, gate, mechanism, transform, endpoint_cells, visibility) in &fences {
        let (Ok(a), Ok(b)) = (nodes.get(edge.a()), nodes.get(edge.b())) else {
            continue;
        };
        if definitions.find_fence(edge.definition).is_none() {
            continue;
        }
        let mut entity_commands = commands.entity(entity);
        if !transform {
            entity_commands.insert(calculate_topology_edge_world_transform(
                a.cell, b.cell, *grid,
            ));
        }
        if !endpoint_cells {
            entity_commands.insert(FenceTerrainEndpointPresentationCells {
                first: a.cell,
                second: b.cell,
            });
        }
        if !visibility {
            entity_commands.insert(Visibility::Inherited);
        }
        if gate && !mechanism {
            entity_commands.insert(GateMechanism::default());
        }
    }

    for (entity, path, surface, transform) in &paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        let mut entity_commands = commands.entity(entity);
        if !transform {
            entity_commands.insert(Transform::from_translation(
                grid.cell_translation(path.cell),
            ));
        }
        if !surface {
            entity_commands.insert(TileSurface {
                height_cm: definition.surface.height_cm,
            });
        }
    }
}

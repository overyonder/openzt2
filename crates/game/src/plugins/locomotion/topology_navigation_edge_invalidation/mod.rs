use bevy::prelude::*;

use crate::plugins::topology::{
    gate_operation_types::Gate,
    topology_edit_types::TopologyChanged,
    topology_graph_types::{FenceEdge, TopologyNode},
};

use super::{
    locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Destination, NavigationOverlay, Route,
    },
    terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
};

/// Projects live gate state into the sparse edge overlay and dirties only
/// routes whose remaining polyline intersects a changed topology rectangle.
pub(super) fn synchronize_navigation_edge_blocking_and_invalidate_routes_after_topology_changes(
    mut topology_changes: MessageReader<TopologyChanged>,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    mut navigation_edge_overlay: ResMut<NavigationOverlay>,
    gate_edges: Query<(&Gate, &FenceEdge)>,
    topology_nodes: Query<&TopologyNode>,
    mut active_routes: Query<(&GlobalTransform, &Route, &mut Destination)>,
) {
    let Some(active_navigation_graph) = active_navigation_graph else {
        for _ in topology_changes.read() {}
        return;
    };
    let terrain_derived_navigation_graph =
        active_navigation_graph.terrain_derived_navigation_graph();
    for topology_change in topology_changes.read() {
        synchronize_closed_gate_blocking_in_navigation_edge_overlay(
            terrain_derived_navigation_graph,
            topology_change.bounds,
            &gate_edges,
            &topology_nodes,
            &mut navigation_edge_overlay,
        );
        for (route_start_transform, route, mut destination) in &mut active_routes {
            if remaining_route_intersects_changed_topology_rectangle(
                route_start_transform.translation(),
                route,
                topology_change.bounds,
            ) {
                destination.set_changed();
            }
        }
    }
}

fn synchronize_closed_gate_blocking_in_navigation_edge_overlay(
    terrain_derived_navigation_graph: &TerrainDerivedNavigationGraph,
    changed_topology_bounds: IRect,
    gate_edges: &Query<(&Gate, &FenceEdge)>,
    topology_nodes: &Query<&TopologyNode>,
    navigation_edge_overlay: &mut NavigationOverlay,
) {
    let Ok(navigation_node_count) =
        u32::try_from(terrain_derived_navigation_graph.navigation_node_count())
    else {
        return;
    };
    for source_navigation_node in 0..navigation_node_count {
        for edge in terrain_derived_navigation_graph
            .iterate_outgoing_navigation_graph_edges(source_navigation_node)
        {
            let destination_navigation_node = edge.destination_node;
            let edge_bounds = calculate_navigation_edge_world_cell_bounds(
                terrain_derived_navigation_graph,
                source_navigation_node,
                destination_navigation_node,
            );
            if !inclusive_rectangles_intersect(edge_bounds, changed_topology_bounds) {
                continue;
            }
            let blocked_by_closed_gate = gate_edges.iter().any(|(gate, fence_edge)| {
                if gate.open {
                    return false;
                }
                let (Ok(first_topology_node), Ok(second_topology_node)) = (
                    topology_nodes.get(fence_edge.a),
                    topology_nodes.get(fence_edge.b),
                ) else {
                    return false;
                };
                inclusive_rectangles_intersect(
                    edge_bounds,
                    create_inclusive_rectangle_from_two_cells(
                        first_topology_node.cell.xy(),
                        second_topology_node.cell.xy(),
                    ),
                )
            });
            navigation_edge_overlay.set_blocked(
                source_navigation_node,
                destination_navigation_node,
                blocked_by_closed_gate,
            );
        }
    }
}

fn remaining_route_intersects_changed_topology_rectangle(
    route_start: Vec3,
    route: &Route,
    changed_topology_bounds: IRect,
) -> bool {
    let mut previous_route_position = route_start;
    for &next_route_position in route.points.get(route.cursor..).unwrap_or(&[]) {
        if inclusive_rectangles_intersect(
            calculate_world_segment_cell_bounds(previous_route_position, next_route_position),
            changed_topology_bounds,
        ) {
            return true;
        }
        previous_route_position = next_route_position;
    }
    false
}

fn calculate_navigation_edge_world_cell_bounds(
    terrain_derived_navigation_graph: &TerrainDerivedNavigationGraph,
    source_navigation_node: u32,
    destination_navigation_node: u32,
) -> IRect {
    let Some((source_position_centimetres, destination_position_centimetres)) =
        terrain_derived_navigation_graph
            .calculate_navigation_node_world_position_centimetres(source_navigation_node)
            .zip(
                terrain_derived_navigation_graph
                    .calculate_navigation_node_world_position_centimetres(
                        destination_navigation_node,
                    ),
            )
    else {
        return IRect::EMPTY;
    };
    create_inclusive_rectangle_from_two_cells(
        IVec2::new(
            source_position_centimetres[0] / 100,
            source_position_centimetres[2] / 100,
        ),
        IVec2::new(
            destination_position_centimetres[0] / 100,
            destination_position_centimetres[2] / 100,
        ),
    )
}

fn calculate_world_segment_cell_bounds(first_position: Vec3, second_position: Vec3) -> IRect {
    create_inclusive_rectangle_from_two_cells(
        IVec2::new(
            first_position.x.floor() as i32,
            first_position.z.floor() as i32,
        ),
        IVec2::new(
            second_position.x.floor() as i32,
            second_position.z.floor() as i32,
        ),
    )
}

fn create_inclusive_rectangle_from_two_cells(first_cell: IVec2, second_cell: IVec2) -> IRect {
    IRect::from_corners(first_cell.min(second_cell), first_cell.max(second_cell))
}

fn inclusive_rectangles_intersect(first_rectangle: IRect, second_rectangle: IRect) -> bool {
    first_rectangle.min.cmple(second_rectangle.max).all()
        && second_rectangle.min.cmple(first_rectangle.max).all()
}

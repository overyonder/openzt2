use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::horizontal_world_segments_intersect;

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationOverlay;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

/// Projects references to crossed fences into the sparse edge overlay and dirties only
/// routes whose remaining polyline intersects a changed topology rectangle.
pub(super) fn synchronize_navigation_edge_blocking_and_invalidate_routes_after_topology_changes(
    mut topology_changes: MessageReader<TopologyChanged>,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    mut navigation_edge_overlay: ResMut<NavigationOverlay>,
    fence_edges: Query<(Entity, &FenceEdge)>,
    topology_grid: Res<TopologyGrid>,
    topology_nodes: Query<&TopologyNode>,
    mut active_routes: Query<(&GlobalTransform, &Route, &mut Destination)>,
) {
    let Some(active_navigation_graph) = active_navigation_graph else {
        for _ in topology_changes.read() {}
        return;
    };
    let terrain_derived_navigation_graph =
        active_navigation_graph.terrain_derived_navigation_graph();
    let mut rebuild_crossings = active_navigation_graph.is_changed();
    for topology_change in topology_changes.read() {
        rebuild_crossings = true;
        let changed_world_bounds =
            calculate_topology_rectangle_world_cell_bounds(topology_change.bounds, *topology_grid);
        for (route_start_transform, route, mut destination) in &mut active_routes {
            if remaining_route_intersects_changed_topology_rectangle(
                route_start_transform.translation(),
                route,
                changed_world_bounds,
            ) {
                destination.set_changed();
            }
        }
    }
    if rebuild_crossings {
        synchronize_fence_crossings_in_navigation_edge_overlay(
            terrain_derived_navigation_graph,
            &fence_edges,
            &topology_nodes,
            *topology_grid,
            &mut navigation_edge_overlay,
        );
    }
}

fn synchronize_fence_crossings_in_navigation_edge_overlay(
    terrain_derived_navigation_graph: &TerrainDerivedNavigationGraph,
    fence_edges: &Query<(Entity, &FenceEdge)>,
    topology_nodes: &Query<&TopologyNode>,
    topology_grid: TopologyGrid,
    navigation_edge_overlay: &mut NavigationOverlay,
) {
    let cell_size =
        f32::from(terrain_derived_navigation_graph.terrain_cell_size_centimetres) * 0.01;
    if cell_size <= 0.0 {
        return;
    }
    let dimensions = terrain_derived_navigation_graph.terrain_tile_grid_dimensions;
    let cells_per_tile = u32::from(terrain_derived_navigation_graph.terrain_cells_per_tile);
    let Some(maximum_cell) = dimensions[0]
        .checked_mul(cells_per_tile)
        .and_then(|count| count.checked_sub(1))
        .and_then(|cell| i32::try_from(cell).ok())
        .zip(
            dimensions[1]
                .checked_mul(cells_per_tile)
                .and_then(|count| count.checked_sub(1))
                .and_then(|cell| i32::try_from(cell).ok()),
        )
        .map(|(x, y)| IVec2::new(x, y))
    else {
        return;
    };
    let mut crossings = HashMap::<(u32, u32), Vec<Entity>>::new();
    // Graph edges join adjacent terrain cells. Visit only each fence's small
    // cell rectangle, rather than comparing every graph edge to every fence.
    for (entity, fence) in fence_edges.iter() {
        let (Ok(a), Ok(b)) = (topology_nodes.get(fence.a), topology_nodes.get(fence.b)) else {
            continue;
        };
        let a = topology_grid.cell_translation(a.cell).xz();
        let b = topology_grid.cell_translation(b.cell).xz();
        let a_cell = Vec2::new(a.x, -a.y) / cell_size;
        let b_cell = Vec2::new(b.x, -b.y) / cell_size;
        let minimum = (a_cell.min(b_cell).floor().as_ivec2() - IVec2::ONE).max(IVec2::ZERO);
        let maximum = (a_cell.max(b_cell).floor().as_ivec2() + IVec2::ONE).min(maximum_cell);
        for y in minimum.y..=maximum.y {
            for x in minimum.x..=maximum.x {
                let Some(source_navigation_node) = terrain_derived_navigation_graph
                    .calculate_navigation_node_for_terrain_cell([
                        u32::try_from(x).expect("clamped terrain column"),
                        u32::try_from(y).expect("clamped terrain row"),
                    ])
                else {
                    continue;
                };
                for edge in terrain_derived_navigation_graph
                    .iterate_outgoing_navigation_graph_edges(source_navigation_node)
                {
                    let destination_navigation_node = edge.destination_node;
                    let Some((source, destination)) = terrain_derived_navigation_graph
                        .calculate_navigation_node_world_position_centimetres(
                            source_navigation_node,
                        )
                        .zip(
                            terrain_derived_navigation_graph
                                .calculate_navigation_node_world_position_centimetres(
                                    destination_navigation_node,
                                ),
                        )
                    else {
                        continue;
                    };
                    let source = Vec2::new(source[0] as f32, source[2] as f32) * 0.01;
                    let destination =
                        Vec2::new(destination[0] as f32, destination[2] as f32) * 0.01;
                    if horizontal_world_segments_intersect(source, destination, a, b) {
                        crossings
                            .entry((source_navigation_node, destination_navigation_node))
                            .or_default()
                            .push(entity);
                    }
                }
            }
        }
    }
    navigation_edge_overlay.replace_fence_crossings(crossings);
}

fn calculate_topology_rectangle_world_cell_bounds(bounds: IRect, grid: TopologyGrid) -> IRect {
    calculate_world_segment_cell_bounds(
        grid.cell_translation(bounds.min.extend(0)),
        grid.cell_translation(bounds.max.extend(0)),
    )
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

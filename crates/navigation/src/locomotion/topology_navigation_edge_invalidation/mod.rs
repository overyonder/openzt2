use bevy::ecs::{entity::EntityHashSet, system::SystemParam};
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_graph_types::{
    FenceEdge, FenceEnd, FenceSegment, FenceStart, FencesEndingHere, FencesStartingHere,
};
use openzt2_zoo_state::topology::topology_grid_geometry::horizontal_world_segments_intersect;

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

use super::fence_navigation_crossing_types::*;

/// Dirties routes whose remaining polyline intersects a changed topology rectangle.
pub(super) fn invalidate_routes_after_topology_changes(
    mut topology_changes: MessageReader<TopologyChanged>,
    active_navigation_graph: Option<Single<&ActiveTerrainDerivedNavigationGraph>>,
    topology_grid: Single<&TopologyGrid>,
    mut active_routes: Query<(&GlobalTransform, &Route, &mut Destination)>,
) {
    let active_navigation_graph = active_navigation_graph.map(Single::into_inner);
    let topology_grid = topology_grid.into_inner();
    let bounds: Vec<_> = topology_changes
        .read()
        .map(|change| calculate_topology_rectangle_world_cell_bounds(change.bounds, *topology_grid))
        .collect();
    if bounds.is_empty() || active_navigation_graph.is_none() {
        return;
    }
    active_routes
        .par_iter_mut()
        .for_each(|(transform, route, mut destination)| {
            if bounds.iter().copied().any(|bounds| {
                remaining_route_intersects_changed_topology_rectangle(
                    transform.translation(),
                    route,
                    bounds,
                )
            }) {
                destination.set_changed();
            }
        });
}

#[derive(SystemParam)]
pub(super) struct FenceNavigationCrossingSources<'w, 's> {
    fences: Query<'w, 's, (FenceSegment, &'static WorldMember)>,
    world_fences: Query<'w, 's, (Entity, &'static WorldMember), With<FenceEdge>>,
    changed_fences:
        Query<'w, 's, Entity, Or<(Changed<FenceEdge>, Changed<FenceStart>, Changed<FenceEnd>)>>,
    incident: Query<
        'w,
        's,
        (
            Option<&'static FencesStartingHere>,
            Option<&'static FencesEndingHere>,
        ),
    >,
    nodes: Query<'w, 's, &'static TopologyNode>,
    changed_nodes: Query<'w, 's, Entity, Changed<TopologyNode>>,
    removed_nodes: RemovedComponents<'w, 's, TopologyNode>,
    memberships: Query<'w, 's, &'static FenceCrossings>,
    crossings: Query<'w, 's, &'static CrossingOfEdge>,
    numeric_edges: Query<'w, 's, &'static NavigationEdgeKey>,
}

pub(super) fn synchronize_fence_navigation_crossings(
    mut commands: Commands,
    roots: Query<(
        Entity,
        Option<&ActiveTerrainDerivedNavigationGraph>,
        Ref<TopologyGrid>,
        &FenceNavigationCrossingIndex,
        Option<&TopologyIndex>,
    )>,
    mut sources: FenceNavigationCrossingSources,
    mut installations: MessageReader<super::world_navigation_installation::NavigationGraphReplaced>,
    mut path_changes: MessageReader<
        super::path_surface_navigation_projection::NavigationPathGraphChanged,
    >,
    mut terrain_changes: MessageReader<
        super::terrain_edit_navigation_refresh::NavigationTerrainGraphChanged,
    >,
    mut removed_graphs: RemovedComponents<ActiveTerrainDerivedNavigationGraph>,
    mut crossed_edges: Local<Vec<DirectedNavigationEdge>>,
) {
    let mut refresh_roots: EntityHashSet = installations
        .read()
        .map(|event| event.0)
        .chain(
            path_changes
                .read()
                .filter(|change| change.elevated)
                .map(|change| change.root),
        )
        .chain(removed_graphs.read())
        .collect();
    refresh_roots.extend(
        roots
            .iter()
            .filter_map(|(root, _, grid, _, _)| grid.is_changed().then_some(root)),
    );
    let nodes = sources
        .changed_nodes
        .iter()
        .chain(sources.removed_nodes.read());
    let mut affected: EntityHashSet = sources.changed_fences.iter().collect();
    for edit in terrain_changes.read() {
        let Ok((_, graph, grid, _, index)) = roots.get(edit.root) else {
            continue;
        };
        let Some(index) = index else {
            refresh_roots.insert(edit.root);
            continue;
        };
        if refresh_roots.contains(&edit.root) || grid.spacing_m <= 0.0 {
            continue;
        }
        let padding = graph
            .map(ActiveTerrainDerivedNavigationGraph::terrain_derived_navigation_graph)
            .map_or(grid.spacing_m, |graph| {
                // An outgoing edge may start in the neighboring navigation cell.
                f32::from(graph.terrain_cell_size_centimetres) * 0.02 + grid.spacing_m
            });
        let first = ((edit.bounds.min - grid.origin - Vec2::splat(padding)) / grid.spacing_m)
            .floor()
            .as_ivec2();
        let last = ((edit.bounds.max - grid.origin + Vec2::splat(padding)) / grid.spacing_m)
            .ceil()
            .as_ivec2();
        affected.extend(index.fences_within(first, last));
    }
    for node in nodes {
        let Ok((outgoing, incoming)) = sources.incident.get(node) else {
            continue;
        };
        affected.extend(outgoing.into_iter().flat_map(|edges| edges.iter()));
        affected.extend(incoming.into_iter().flat_map(|edges| edges.iter()));
    }
    if !refresh_roots.is_empty() {
        affected.extend(
            sources.world_fences.iter().filter_map(|(fence, member)| {
                refresh_roots.contains(&member.root).then_some(fence)
            }),
        );
    }
    let mut new_edges = bevy::platform::collections::HashMap::new();
    for fence_entity in affected {
        let Ok((fence, member)) = sources.fences.get(fence_entity) else {
            continue;
        };
        let Ok((_, graph, grid, index, _)) = roots.get(member.root) else {
            continue;
        };
        crossed_edges.clear();
        if let Some(graph) =
            graph.map(ActiveTerrainDerivedNavigationGraph::terrain_derived_navigation_graph)
        {
            if let (Some((cell_size, maximum_cell)), Ok(a), Ok(b)) = (
                navigation_cell_bounds(graph),
                sources.nodes.get(fence.a()),
                sources.nodes.get(fence.b()),
            ) {
                collect_crossed_navigation_edges(
                    graph,
                    cell_size,
                    maximum_cell,
                    grid.cell_translation(a.cell).xz(),
                    grid.cell_translation(b.cell).xz(),
                    &mut crossed_edges,
                );
            }
        }
        let mut existing = bevy::platform::collections::HashSet::new();
        if let Ok(links) = sources.memberships.get(fence_entity) {
            for link in links.iter() {
                let key = sources
                    .crossings
                    .get(link)
                    .ok()
                    .and_then(|edge| sources.numeric_edges.get(edge.0).ok())
                    .map(|edge| edge.0);
                if key.is_some_and(|key| crossed_edges.binary_search(&key).is_ok()) {
                    existing.extend(key);
                } else {
                    commands.entity(link).try_despawn();
                }
            }
        }
        for &key in crossed_edges.iter().filter(|&key| !existing.contains(key)) {
            let edge = index.0.get(&key).copied().unwrap_or_else(|| {
                *new_edges
                    .entry((member.root, key))
                    .or_insert_with(|| commands.spawn((*member, NavigationEdgeKey(key))).id())
            });
            commands.spawn((CrossingOfFence(fence_entity), CrossingOfEdge(edge)));
        }
    }
}

pub(super) fn remove_crossings_after_fence_is_removed(
    removed: On<Remove<FenceEdge>>,
    memberships: Query<&FenceCrossings>,
    mut commands: Commands,
) {
    if let Ok(links) = memberships.get(removed.entity) {
        for link in links.iter() {
            commands.entity(link).try_despawn();
        }
    }
}

fn navigation_cell_bounds(graph: &TerrainDerivedNavigationGraph) -> Option<(f32, IVec2)> {
    let cell_size = f32::from(graph.terrain_cell_size_centimetres) * 0.01;
    if cell_size <= 0.0 {
        return None;
    }
    let [columns, rows] = graph.terrain_tile_grid_dimensions;
    let cells_per_tile = u32::from(graph.terrain_cells_per_tile);
    let last_cell = |tiles: u32| {
        tiles
            .checked_mul(cells_per_tile)
            .and_then(|count| count.checked_sub(1))
            .and_then(|cell| i32::try_from(cell).ok())
    };
    Some((cell_size, IVec2::new(last_cell(columns)?, last_cell(rows)?)))
}

fn collect_crossed_navigation_edges(
    graph: &TerrainDerivedNavigationGraph,
    cell_size: f32,
    maximum_cell: IVec2,
    a: Vec2,
    b: Vec2,
    crossings: &mut Vec<DirectedNavigationEdge>,
) {
    let a_cell = Vec2::new(a.x, -a.y) / cell_size;
    let b_cell = Vec2::new(b.x, -b.y) / cell_size;
    let minimum = (a_cell.min(b_cell).floor().as_ivec2() - IVec2::ONE).max(IVec2::ZERO);
    let maximum = (a_cell.max(b_cell).floor().as_ivec2() + IVec2::ONE).min(maximum_cell);
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let Some(from) = graph.calculate_navigation_node_for_terrain_cell([
                u32::try_from(x).expect("clamped terrain column"),
                u32::try_from(y).expect("clamped terrain row"),
            ]) else {
                continue;
            };
            let Some(source) = graph.calculate_navigation_node_world_position_centimetres(from)
            else {
                continue;
            };
            let source = Vec2::new(source[0] as f32, source[2] as f32) * 0.01;
            for edge in graph.iterate_outgoing_navigation_graph_edges(from, None) {
                let to = edge.destination_node;
                let Some(destination) =
                    graph.calculate_navigation_node_world_position_centimetres(to)
                else {
                    continue;
                };
                let destination = Vec2::new(destination[0] as f32, destination[2] as f32) * 0.01;
                if horizontal_world_segments_intersect(source, destination, a, b) {
                    crossings.push((from, to));
                }
            }
        }
    }
    crossings.sort_unstable();
    crossings.dedup();
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

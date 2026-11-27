use super::changed_destination_route_planning::{ActiveRouteSearch, InstalledRouteEdges};
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_navigation_cell_queries::read_navigation_cell_geometry_from_terrain_samples;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{
    SnapshotTerrainNavigationPending, SnapshotTerrainNavigationReady, TerrainNavigationRestored,
};

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::{
    NavigationEdgeChange, NavigationTerrainCellEdit, NavigationTerrainChanges,
    TerrainDerivedNavigationGraph,
};

/// Refreshes restored terrain before saved actor routes are applied.
pub(super) fn refresh_navigation_graph_after_terrain_restoration(
    mut restored_terrain: MessageReader<TerrainNavigationRestored>,
    mut graphs: Query<&mut ActiveTerrainDerivedNavigationGraph>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>, &WorldMember)>,
) {
    for restored in restored_terrain.read() {
        let Ok((chunk, edited, member)) = terrain_chunks.get(restored.chunk) else {
            continue;
        };
        let Ok(mut graph) = graphs.get_mut(member.root) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            graph.terrain_derived_navigation_graph_mut(),
            chunk,
            terrain,
            edited,
            restored.min,
            restored.max,
        );
    }
}

pub(super) fn acknowledge_restored_terrain_navigation(
    mut pending: MessageReader<SnapshotTerrainNavigationPending>,
    mut ready: MessageWriter<SnapshotTerrainNavigationReady>,
    graphs: Query<(), With<ActiveTerrainDerivedNavigationGraph>>,
) {
    for pending in pending
        .read()
        .filter(|pending| graphs.contains(pending.root))
    {
        ready.write(SnapshotTerrainNavigationReady {
            root: pending.root,
            operation: pending.operation,
        });
    }
}

/// Refreshes edited cells before the next route-planning pass.
// Bevy system parameters are consumed by value when the system is registered.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn refresh_navigation_graph_after_terrain_edits(
    mut terrain_navigation_changes: MessageReader<TerrainNavigationChanged>,
    mut graphs: Query<&mut ActiveTerrainDerivedNavigationGraph>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>, &WorldMember)>,
    mut changed: MessageWriter<NavigationTerrainGraphChanged>,
) {
    for terrain_change in terrain_navigation_changes.read() {
        let Ok((terrain_chunk, edited_terrain_samples, member)) =
            terrain_chunks.get(terrain_change.chunk)
        else {
            continue;
        };
        let Some(terrain_asset) = terrain_assets.get(&terrain_chunk.asset) else {
            continue;
        };
        let Ok(mut graph) = graphs.get_mut(member.root) else {
            continue;
        };
        let changes = refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            graph.terrain_derived_navigation_graph_mut(),
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            terrain_change.min,
            terrain_change.max,
        );
        if !changes.affected_nodes.is_empty() {
            changed.write(NavigationTerrainGraphChanged {
                root: member.root,
                bounds: navigation_sample_edit_bounds(
                    terrain_chunk,
                    terrain_change.min,
                    terrain_change.max,
                ),
                changes,
            });
        }
    }
}

#[derive(Message)]
pub(super) struct NavigationTerrainGraphChanged {
    pub(super) root: Entity,
    pub(super) bounds: Rect,
    pub(super) changes: NavigationTerrainChanges,
}

pub(super) fn navigation_sample_edit_bounds(chunk: &TerrainChunk, min: UVec2, max: UVec2) -> Rect {
    let min = chunk.origin + min.saturating_sub(UVec2::ONE).as_vec2() * chunk.spacing_m;
    let max = chunk.origin + (max.as_vec2() + Vec2::ONE) * chunk.spacing_m;
    Rect::from_corners(min, max)
}

pub(super) fn invalidate_routes_after_navigation_geometry_changes(
    mut terrain_changes: MessageReader<NavigationTerrainGraphChanged>,
    mut path_changes: MessageReader<
        super::path_surface_navigation_projection::NavigationPathGraphChanged,
    >,
    mut water_changes: MessageReader<
        super::tank_water_navigation_overlay::TankNavigationWaterChanged,
    >,
    graphs: Query<&ActiveTerrainDerivedNavigationGraph>,
    mut installations: MessageReader<super::world_navigation_installation::NavigationGraphReplaced>,
    mut routes: Query<(
        &mut Route,
        &mut Destination,
        &WorldMember,
        &GlobalTransform,
        Option<&InstalledRouteEdges>,
        Option<&mut ActiveRouteSearch>,
    )>,
) {
    let mut changed = bevy::ecs::entity::EntityHashMap::<Vec<Rect>>::default();
    let mut water_changed = bevy::ecs::entity::EntityHashMap::<Vec<Rect>>::default();
    let mut rebuilt: bevy::ecs::entity::EntityHashSet =
        installations.read().map(|event| event.0).collect();
    let mut edge_deltas =
        bevy::ecs::entity::EntityHashMap::<Vec<([u32; 2], NavigationEdgeChange)>>::default();
    let mut changed_nodes = bevy::ecs::entity::EntityHashMap::<Vec<u32>>::default();
    for event in terrain_changes.read() {
        changed.entry(event.root).or_default().push(event.bounds);
        edge_deltas.entry(event.root).or_default().extend(
            event
                .changes
                .affected_edges
                .iter()
                .map(|(&edge, &delta)| (edge, delta)),
        );
        changed_nodes
            .entry(event.root)
            .or_default()
            .extend(event.changes.affected_nodes.keys().copied());
    }
    let record_cells =
        |changed: &mut bevy::ecs::entity::EntityHashMap<Vec<Rect>>, root, cells: &[[u32; 2]]| {
            let Ok(graph) = graphs.get(root) else {
                return;
            };
            let size = f32::from(
                graph
                    .terrain_derived_navigation_graph()
                    .terrain_cell_size_centimetres,
            ) * 0.01;
            let bounds = changed.entry(root).or_default();
            bounds.extend(cells.iter().map(|&[x, z]| {
                let corner = Vec2::new(x as f32, -(z as f32)) * size;
                Rect::from_corners(corner, corner + Vec2::new(size, -size))
            }));
        };
    for event in path_changes.read() {
        edge_deltas.entry(event.root).or_default().extend(
            event
                .changes
                .affected_edges
                .iter()
                .map(|(&edge, &delta)| (edge, delta)),
        );
        let cells: Vec<_> = event.changes.affected_cells.iter().copied().collect();
        record_cells(&mut changed, event.root, &cells);
    }
    for event in water_changes.read() {
        if let Some(cells) = &event.cells {
            record_cells(&mut water_changed, event.root, cells);
        } else {
            rebuilt.insert(event.root);
        }
    }
    if changed.is_empty()
        && water_changed.is_empty()
        && rebuilt.is_empty()
        && edge_deltas.is_empty()
    {
        return;
    }
    routes.par_iter_mut().for_each(
        |(mut route, mut destination, member, transform, installed, active)| {
            let edges = edge_deltas
                .get(&member.root)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let nodes = changed_nodes
                .get(&member.root)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let graph_replaced = rebuilt.contains(&member.root);
            if let Some(mut active) = active {
                if graph_replaced || active.is_affected_by(edges, nodes) {
                    active.reset();
                }
                return;
            }
            let installed =
                installed.filter(|installed| installed.request == destination.request_id);
            let overlaps = |bounds: &[Rect]| {
                let mut from = transform.translation().xz();
                route.points.iter().skip(route.cursor).any(|point| {
                    let to = point.xz();
                    let intersects = bounds
                        .iter()
                        .any(|&area| segment_intersects_rect(from, to, area));
                    from = to;
                    intersects
                })
            };
            let affected = installed.map_or_else(
                || {
                    changed
                        .get(&member.root)
                        .is_some_and(|bounds| overlaps(bounds))
                },
                |installed| installed.is_affected_by(edges, nodes),
            ) || water_changed
                .get(&member.root)
                .is_some_and(|bounds| overlaps(bounds));
            if affected || graph_replaced {
                route.clear();
                destination.set_changed();
            }
        },
    );
}

fn segment_intersects_rect(from: Vec2, to: Vec2, rect: Rect) -> bool {
    let delta = to - from;
    let mut first = 0.0_f32;
    let mut last = 1.0_f32;
    for axis in 0..2 {
        if delta[axis] == 0.0 {
            if from[axis] < rect.min[axis] || from[axis] > rect.max[axis] {
                return false;
            }
            continue;
        }
        let a = (rect.min[axis] - from[axis]) / delta[axis];
        let b = (rect.max[axis] - from[axis]) / delta[axis];
        first = first.max(a.min(b));
        last = last.min(a.max(b));
        if first > last {
            return false;
        }
    }
    true
}

pub(super) fn refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
    terrain_derived_navigation_graph: &mut TerrainDerivedNavigationGraph,
    terrain_chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
    edited_terrain_samples: Option<&EditedTerrainSamples>,
    changed_sample_minimum: UVec2,
    changed_sample_maximum: UVec2,
) -> NavigationTerrainChanges {
    let Some(terrain_cells_per_chunk_side) = terrain_chunk.side.checked_sub(1).map(u32::from)
    else {
        return NavigationTerrainChanges::default();
    };
    let Some(last_chunk_cell_index) = terrain_cells_per_chunk_side.checked_sub(1) else {
        return NavigationTerrainChanges::default();
    };
    let minimum_terrain_cell = UVec2::new(
        changed_sample_minimum.x.saturating_sub(1),
        changed_sample_minimum.y.saturating_sub(1),
    )
    .min(UVec2::splat(last_chunk_cell_index));
    let maximum_terrain_cell = changed_sample_maximum.min(UVec2::splat(last_chunk_cell_index));
    if minimum_terrain_cell.cmpgt(maximum_terrain_cell).any() {
        return NavigationTerrainChanges::default();
    }
    let Ok(terrain_chunk_x) = u32::try_from(terrain_chunk.source_coord.x) else {
        return NavigationTerrainChanges::default();
    };
    let Ok(terrain_chunk_z) = u32::try_from(terrain_chunk.source_coord.y) else {
        return NavigationTerrainChanges::default();
    };
    let Some(terrain_chunk_cell_origin) = terrain_chunk_x
        .checked_mul(terrain_cells_per_chunk_side)
        .zip(terrain_chunk_z.checked_mul(terrain_cells_per_chunk_side))
        .map(|(x, z)| UVec2::new(x, z))
    else {
        return NavigationTerrainChanges::default();
    };
    let mut edits = Vec::new();
    for z in minimum_terrain_cell.y..=maximum_terrain_cell.y {
        for x in minimum_terrain_cell.x..=maximum_terrain_cell.x {
            let local_terrain_cell = UVec2::new(x, z);
            let Some((
                height_centimetres,
                contains_water,
                depth_centimetres,
                minimum_triangle_normal_y,
            )) = read_navigation_cell_geometry_from_terrain_samples(
                terrain_chunk,
                terrain_asset,
                edited_terrain_samples,
                local_terrain_cell,
            )
            else {
                continue;
            };
            let Some(source_local_terrain_cell_z) =
                last_chunk_cell_index.checked_sub(local_terrain_cell.y)
            else {
                continue;
            };
            let Some(global_terrain_cell) = terrain_chunk_cell_origin
                .x
                .checked_add(local_terrain_cell.x)
                .zip(
                    terrain_chunk_cell_origin
                        .y
                        .checked_add(source_local_terrain_cell_z),
                )
                .map(|(x, z)| UVec2::new(x, z))
            else {
                continue;
            };
            edits.push(NavigationTerrainCellEdit {
                cell: global_terrain_cell,
                height_centimetres,
                minimum_triangle_normal_y,
                contains_water,
                water_depth_centimetres: depth_centimetres,
            });
        }
    }
    terrain_derived_navigation_graph.apply_terrain_cell_edits(edits, None)
}

use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_navigation_cell_queries::read_navigation_cell_geometry_from_terrain_samples;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationRestored;

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

/// Refreshes restored terrain before saved actor routes are applied.
pub(super) fn refresh_navigation_graph_after_terrain_restoration(
    mut restored_terrain: MessageReader<TerrainNavigationRestored>,
    active_navigation_graph: Option<ResMut<ActiveTerrainDerivedNavigationGraph>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) {
    let Some(mut graph) = active_navigation_graph else {
        return;
    };
    for restored in restored_terrain.read() {
        let Ok((chunk, edited)) = terrain_chunks.get(restored.chunk) else {
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

/// Refreshes edited cells before the next route-planning pass.
// Bevy system parameters are consumed by value when the system is registered.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn refresh_navigation_graph_after_terrain_edits(
    mut terrain_navigation_changes: MessageReader<TerrainNavigationChanged>,
    mut active_navigation_graph: Option<ResMut<ActiveTerrainDerivedNavigationGraph>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut changed: MessageWriter<NavigationTerrainGraphChanged>,
) {
    let mut received_terrain_change = false;
    for terrain_change in terrain_navigation_changes.read() {
        let Some(active_navigation_graph) = active_navigation_graph.as_mut() else {
            continue;
        };
        let Ok((terrain_chunk, edited_terrain_samples)) = terrain_chunks.get(terrain_change.chunk)
        else {
            continue;
        };
        let Some(terrain_asset) = terrain_assets.get(&terrain_chunk.asset) else {
            continue;
        };
        received_terrain_change = true;
        refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            active_navigation_graph.terrain_derived_navigation_graph_mut(),
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            terrain_change.min,
            terrain_change.max,
        );
    }
    if received_terrain_change {
        changed.write(NavigationTerrainGraphChanged);
    }
}

#[derive(Message)]
pub(super) struct NavigationTerrainGraphChanged;

pub(super) fn invalidate_routes_after_navigation_geometry_changes(
    mut terrain_changes: MessageReader<NavigationTerrainGraphChanged>,
    mut path_changes: MessageReader<
        super::path_surface_navigation_projection::NavigationPathGraphChanged,
    >,
    capacity: Option<Res<openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity>>,
    mut routes: Query<(&mut Route, &mut Destination)>,
) {
    let changed = (terrain_changes.read().count() != 0) | (path_changes.read().count() != 0);
    if !changed {
        return;
    }
    let point_capacity = capacity.map_or(0, |capacity| capacity.max_route_points);
    routes
        .par_iter_mut()
        .for_each(|(mut route, mut destination)| {
            route.clear();
            if route.points.capacity() < point_capacity {
                route.points.reserve_exact(point_capacity);
            }
            destination.set_changed();
        });
}

pub(super) fn refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
    terrain_derived_navigation_graph: &mut TerrainDerivedNavigationGraph,
    terrain_chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
    edited_terrain_samples: Option<&EditedTerrainSamples>,
    changed_sample_minimum: UVec2,
    changed_sample_maximum: UVec2,
) {
    let Some(terrain_cells_per_chunk_side) = terrain_chunk.side.checked_sub(1).map(u32::from)
    else {
        return;
    };
    let Some(last_chunk_cell_index) = terrain_cells_per_chunk_side.checked_sub(1) else {
        return;
    };
    let minimum_terrain_cell = UVec2::new(
        changed_sample_minimum.x.saturating_sub(1),
        changed_sample_minimum.y.saturating_sub(1),
    )
    .min(UVec2::splat(last_chunk_cell_index));
    let maximum_terrain_cell = changed_sample_maximum.min(UVec2::splat(last_chunk_cell_index));
    if minimum_terrain_cell.cmpgt(maximum_terrain_cell).any() {
        return;
    }
    let Ok(terrain_chunk_x) = u32::try_from(terrain_chunk.source_coord.x) else {
        return;
    };
    let Ok(terrain_chunk_z) = u32::try_from(terrain_chunk.source_coord.y) else {
        return;
    };
    let Some(terrain_chunk_cell_origin) = terrain_chunk_x
        .checked_mul(terrain_cells_per_chunk_side)
        .zip(terrain_chunk_z.checked_mul(terrain_cells_per_chunk_side))
        .map(|(x, z)| UVec2::new(x, z))
    else {
        return;
    };
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
            let node = terrain_derived_navigation_graph
                .calculate_navigation_node_for_terrain_cell(global_terrain_cell.to_array())
                .expect("indexed terrain cell must exist in the active terrain-derived navigation graph");
            terrain_derived_navigation_graph
                .update_navigation_node_from_changed_terrain_cell(
                    global_terrain_cell,
                    height_centimetres,
                    contains_water,
                    minimum_triangle_normal_y,
                )
                .expect("validated terrain cell");
            terrain_derived_navigation_graph
                .find_navigation_node_mut(node)
                .expect("validated navigation node")
                .water_depth_centimetres = depth_centimetres;
        }
    }
}

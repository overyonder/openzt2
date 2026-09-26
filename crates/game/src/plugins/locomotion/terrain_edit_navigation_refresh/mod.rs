use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
        terrain_navigation_cell_queries::read_navigation_cell_height_and_water_from_terrain_samples,
        terrain_navigation_change_types::TerrainNavigationChanged,
    },
};

use super::{
    locomotion_types::{ActiveTerrainDerivedNavigationGraph, Destination, Route},
    terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
};

/// Refreshes edited cells before the next route-planning pass.
// Bevy system parameters are consumed by value when the system is registered.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn refresh_navigation_graph_and_invalidate_routes_after_terrain_edits(
    mut terrain_navigation_changes: MessageReader<TerrainNavigationChanged>,
    mut active_navigation_graph: Option<ResMut<ActiveTerrainDerivedNavigationGraph>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut active_routes: Query<(&mut Route, &mut Destination)>,
) {
    let mut received_terrain_change = false;
    for terrain_change in terrain_navigation_changes.read() {
        received_terrain_change = true;
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
        refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            active_navigation_graph.terrain_derived_navigation_graph_mut(),
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            terrain_change.min,
            terrain_change.max,
        );
    }
    if !received_terrain_change {
        return;
    }
    for (mut route, mut destination) in &mut active_routes {
        route.clear();
        destination.set_changed();
    }
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
            let Some((height_centimetres, contains_water)) =
                read_navigation_cell_height_and_water_from_terrain_samples(
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
            assert!(
                terrain_derived_navigation_graph
                    .update_navigation_node_from_changed_terrain_cell(
                        global_terrain_cell,
                        height_centimetres,
                        contains_water,
                    )
                    .is_some(),
                "indexed terrain cell must exist in the active terrain-derived navigation graph"
            );
        }
    }
}

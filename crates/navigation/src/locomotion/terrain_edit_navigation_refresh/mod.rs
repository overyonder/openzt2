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
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

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
        changed.write(NavigationTerrainGraphChanged {
            root: member.root,
            bounds: navigation_sample_edit_bounds(
                terrain_chunk,
                terrain_change.min,
                terrain_change.max,
            ),
        });
        refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            graph.terrain_derived_navigation_graph_mut(),
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            terrain_change.min,
            terrain_change.max,
        );
    }
}

#[derive(Message)]
pub(super) struct NavigationTerrainGraphChanged {
    pub(super) root: Entity,
    pub(super) bounds: Rect,
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
    mut routes: Query<(&mut Route, &mut Destination, &WorldMember)>,
) {
    let changed: bevy::ecs::entity::EntityHashSet = terrain_changes
        .read()
        .map(|event| event.root)
        .chain(path_changes.read().map(|event| event.root))
        .chain(water_changes.read().map(|event| event.0))
        .collect();
    if changed.is_empty() {
        return;
    }
    routes
        .par_iter_mut()
        .for_each(|(mut route, mut destination, member)| {
            if !changed.contains(&member.root) {
                return;
            }
            route.clear();
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

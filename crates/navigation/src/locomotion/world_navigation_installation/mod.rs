use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;

use crate::locomotion::terrain_edit_navigation_refresh::refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples;
use openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialCellIndex, SpatialGrid};
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct TerrainDerivedNavigationGraphInstalled;

#[derive(Message)]
pub(super) struct NavigationGraphReplaced;

/// Derives the pathfinding index from the selected terrain after the terrain
/// asset and authored world bounds are available.
// Bevy system parameters are consumed by value when the system is registered.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn install_terrain_derived_navigation_graph_for_loaded_world(
    mut commands: Commands,
    mut terrain_asset_events: MessageReader<AssetEvent<TerrainAsset>>,
    mut installations: MessageWriter<NavigationGraphReplaced>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    previous_grid: Res<SpatialGrid>,
    mut loaded_world_terrain_sources: Query<
        (
            Entity,
            &SelectedWorldTerrainAssetHandle,
            &WorldTerrainHorizontalBounds,
            Has<TerrainDerivedNavigationGraphInstalled>,
            Option<&mut TankNavigationWater>,
        ),
        With<WorldRoot>,
    >,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut active_routes: Query<(&mut Route, &mut Destination)>,
) {
    let Ok((world_root_entity, world_terrain, world_bounds, navigation_graph_installed, water)) =
        loaded_world_terrain_sources.single_mut()
    else {
        for _ in terrain_asset_events.read() {}
        return;
    };
    let terrain_asset_reloaded = terrain_asset_events
        .read()
        .any(|event| event.is_modified(world_terrain.0.id()));
    for _ in terrain_asset_events.read() {}
    if navigation_graph_installed && !terrain_asset_reloaded {
        return;
    }
    let Some(terrain_asset) = terrain_assets.get(&world_terrain.0) else {
        return;
    };
    let Some(mut terrain_derived_navigation_graph) =
        TerrainDerivedNavigationGraph::build_from_authored_terrain_grid(
            terrain_asset.canonical_terrain_grid(),
        )
    else {
        return;
    };
    for (terrain_chunk, edited_terrain_samples) in &terrain_chunks {
        if terrain_chunk.asset.id() != world_terrain.0.id() || edited_terrain_samples.is_none() {
            continue;
        }
        let maximum_changed_sample = UVec2::splat(u32::from(terrain_chunk.side.saturating_sub(1)));
        refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples(
            &mut terrain_derived_navigation_graph,
            terrain_chunk,
            terrain_asset,
            edited_terrain_samples,
            UVec2::ZERO,
            maximum_changed_sample,
        );
    }
    let locomotion_capacity = LocomotionCapacity {
        max_agents: terrain_derived_navigation_graph.navigation_node_count(),
        max_route_points: terrain_derived_navigation_graph
            .navigation_node_count()
            .saturating_add(1),
    };
    if !locomotion_capacity.is_valid() {
        return;
    }
    let terrain_tile_grid_dimensions =
        terrain_derived_navigation_graph.terrain_tile_grid_dimensions();
    // Terrain-derived graph construction bounds this centimetre value to u32.
    // Spatial indexing uses Bevy's f32 representation.
    #[allow(clippy::cast_precision_loss)]
    let terrain_tile_size_metres =
        terrain_derived_navigation_graph.terrain_tile_size_centimetres() as f32 * 0.01;
    let [width, height] = terrain_tile_grid_dimensions;
    let Some(cell_count) = width.checked_mul(height).filter(|&count| count != 0) else {
        return;
    };
    for &cell in &previous_grid.cells {
        commands.entity(cell).try_despawn();
    }
    let navigation_spatial_grid = SpatialGrid {
        origin: world_bounds.min,
        cell_size_m: terrain_tile_size_metres,
        width,
        height,
        cells: (0..cell_count)
            .map(|index| {
                commands
                    .spawn((ChildOf(world_root_entity), SpatialCellIndex(index)))
                    .id()
            })
            .collect::<Vec<_>>(),
    };
    if navigation_graph_installed {
        for (mut route, mut destination) in &mut active_routes {
            route.clear();
            destination.set_changed();
        }
    }
    let next_water =
        TankNavigationWater::new(terrain_derived_navigation_graph.navigation_nodes.len());
    if let Some(mut water) = water {
        *water = next_water;
    } else {
        commands.entity(world_root_entity).insert(next_water);
    }
    commands.insert_resource(ActiveTerrainDerivedNavigationGraph::new(
        terrain_derived_navigation_graph,
    ));
    installations.write(NavigationGraphReplaced);
    commands.insert_resource(locomotion_capacity);
    commands.insert_resource(navigation_spatial_grid);
    commands
        .entity(world_root_entity)
        .insert(TerrainDerivedNavigationGraphInstalled);
}

use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::{
        terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
        world_spawn::{
            selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
            world_membership_types::WorldRoot,
            world_terrain_hydration::WorldTerrainHorizontalBounds,
        },
    },
};

use super::{
    locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Destination, LocomotionCapacity, NavAgent, Route,
        SpatialCell, SpatialGrid,
    },
    terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
    terrain_edit_navigation_refresh::refresh_terrain_derived_navigation_graph_cells_from_changed_terrain_samples,
};

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct TerrainDerivedNavigationGraphInstalled;

/// Derives the pathfinding index from the selected terrain after the terrain
/// asset and authored world bounds are available.
// Bevy system parameters are consumed by value when the system is registered.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn install_terrain_derived_navigation_graph_for_loaded_world(
    mut commands: Commands,
    mut terrain_asset_events: MessageReader<AssetEvent<TerrainAsset>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    loaded_world_terrain_sources: Query<
        (
            Entity,
            &SelectedWorldTerrainAssetHandle,
            &WorldTerrainHorizontalBounds,
            Has<TerrainDerivedNavigationGraphInstalled>,
        ),
        With<WorldRoot>,
    >,
    navigation_agents: Query<(Entity, &GlobalTransform), With<NavAgent>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut active_routes: Query<(&mut Route, &mut Destination)>,
) {
    let Ok((world_root_entity, world_terrain, world_bounds, navigation_graph_installed)) =
        loaded_world_terrain_sources.single()
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
    let mut navigation_spatial_grid = SpatialGrid::default();
    let terrain_tile_grid_dimensions =
        terrain_derived_navigation_graph.terrain_tile_grid_dimensions();
    // Terrain-derived graph construction bounds this centimetre value to u32.
    // Spatial indexing uses Bevy's f32 representation.
    #[allow(clippy::cast_precision_loss)]
    let terrain_tile_size_metres =
        terrain_derived_navigation_graph.terrain_tile_size_centimetres() as f32 * 0.01;
    if !navigation_spatial_grid.reserve_layout(
        world_bounds.min,
        terrain_tile_size_metres,
        terrain_tile_grid_dimensions[0],
        terrain_tile_grid_dimensions[1],
        locomotion_capacity.max_agents,
    ) {
        return;
    }
    for (navigation_agent_entity, navigation_agent_transform) in &navigation_agents {
        let navigation_agent_world_position = navigation_agent_transform.translation();
        commands.entity(navigation_agent_entity).insert(
            navigation_spatial_grid
                .cell_of(Vec2::new(
                    navigation_agent_world_position.x,
                    navigation_agent_world_position.z,
                ))
                .map_or(SpatialCell::OUTSIDE, SpatialCell),
        );
    }
    if navigation_graph_installed {
        for (mut route, mut destination) in &mut active_routes {
            route.clear();
            destination.set_changed();
        }
    }
    commands.insert_resource(ActiveTerrainDerivedNavigationGraph::new(
        terrain_derived_navigation_graph,
    ));
    commands.insert_resource(locomotion_capacity);
    commands.insert_resource(navigation_spatial_grid);
    commands
        .entity(world_root_entity)
        .insert(TerrainDerivedNavigationGraphInstalled);
}

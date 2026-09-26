use bevy::prelude::{
    AssetServer, Assets, Commands, Component, Entity, IVec2, Query, Res, ResMut, Vec2, With,
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::terrain::terrain_chunk_identity_type::TerrainChunkId;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::topology::topology_graph_types::TopologyGrid;

use super::{
    selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
    world_hydration_types::WorldHydration,
    world_load_failure::WorldLoadFailure,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::{WorldMember, WorldRoot},
};

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct WorldTerrainHorizontalBounds {
    pub(crate) min: Vec2,
    pub(crate) max: Vec2,
}

/// Hydrate the selected terrain asset into chunk entities and derive the
/// world's XZ bounds from the authored chunk bounds. Loading cannot finish
/// until this succeeds, so the camera never receives guessed extents.
pub(super) fn hydrate_selected_terrain_chunks_world_bounds_and_topology_grid(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    terrains: Res<Assets<TerrainAsset>>,
    asset_server: Res<AssetServer>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    roots: Query<&SelectedWorldTerrainAssetHandle, With<WorldRoot>>,
    mut pending: Query<(Entity, &mut WorldHydration)>,
) {
    let _performance_timer = performance.measure(WorldLoadingPerformanceStage::TerrainHydration);
    let Ok((root, mut pending)) = pending.single_mut() else {
        return;
    };
    if !pending.terrain_hydration_is_pending() {
        return;
    }
    let Ok(source) = roots.get(root) else {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let Some(terrain) = terrains.get(&source.0) else {
        if matches!(
            asset_server.load_state(source.0.id()),
            bevy::asset::LoadState::Failed(_)
        ) {
            let reason = asset_server.get_path(source.0.id()).map_or(
                WorldLoadFailure::InvalidRecord(0),
                |path| {
                    WorldLoadFailure::MissingDependency(
                        openzt2_game_data::AssetId::from_virtual_path(
                            &path.path().to_string_lossy(),
                        ),
                    )
                },
            );
            pending.record_failure(reason);
        }
        return;
    };
    let Some(side) = terrain
        .terrain_chunk_cells_per_side()
        .and_then(|cells| cells.checked_add(1))
    else {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let spacing_m = terrain.canonical_terrain_grid().units.cell_size_metres;
    if side < 2 || !spacing_m.is_finite() || spacing_m <= 0.0 {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    }
    let Some(topology_spacing_m) = active_definitions
        .get(&definitions)
        .map(WorldDefinitionsView::topology_cell_size_cm)
        .filter(|spacing| *spacing != 0)
        .map(|spacing| f32::from(spacing) * 0.01)
    else {
        return;
    };

    let extent = Vec2::new(
        terrain.canonical_terrain_grid().width.saturating_sub(1) as f32 * spacing_m,
        terrain.canonical_terrain_grid().height.saturating_sub(1) as f32 * spacing_m,
    );
    let min = Vec2::new(0.0, -extent.y);
    let max = Vec2::new(extent.x, 0.0);
    for source_z in 0..terrain.canonical_terrain_grid().sector_rows {
        for x in 0..terrain.canonical_terrain_grid().sector_columns {
            let index = source_z * terrain.canonical_terrain_grid().sector_columns + x;
            let coord = IVec2::new(x as i32, -(source_z as i32) - 1);
            let origin = coord.as_vec2() * f32::from(side - 1) * spacing_m;
            commands.spawn((
                TerrainChunkId(index),
                TerrainChunk {
                    source_coord: IVec2::new(x as i32, source_z as i32),
                    coord,
                    asset: source.0.clone(),
                    origin,
                    spacing_m,
                    side,
                },
                WorldMember { root },
            ));
        }
    }
    commands
        .entity(root)
        .insert(WorldTerrainHorizontalBounds { min, max });
    commands.insert_resource(TopologyGrid {
        origin: min,
        spacing_m: topology_spacing_m,
    });
    pending.record_terrain_hydration_completion();
}

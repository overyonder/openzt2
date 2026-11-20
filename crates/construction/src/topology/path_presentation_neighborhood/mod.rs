use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::{
    terrain::{terrain_chunk_types::TerrainChunk, terrain_edit_types::TerrainChanged},
    topology::{
        ground_path_layout_calculation::calculate_snapped_ground_path_layout,
        topology_graph_types::TopologyGrid,
    },
};

pub(super) fn maximum_path_step(definitions: WorldDefinitionsView<'_>) -> i32 {
    definitions
        .paths()
        .filter_map(|path| {
            calculate_snapped_ground_path_layout(
                IVec3::ZERO,
                path.width_cm,
                definitions.topology_cell_size_cm(),
            )
            .map(|(_, _, step)| step)
        })
        .max()
        .unwrap_or(1)
        .max(1)
}

// Include triangle coverage, the adjacent curve and the partner's far end.
pub(super) fn curb_neighborhood_radius(definitions: WorldDefinitionsView<'_>) -> i32 {
    maximum_path_step(definitions).saturating_mul(6)
}

pub(super) fn terrain_region(
    change: &TerrainChanged,
    chunks: &Query<&TerrainChunk>,
) -> Option<(Vec2, Vec2)> {
    let chunk = chunks.get(change.chunk).ok()?;
    let min = chunk.origin + (change.min.as_vec2() - Vec2::ONE) * chunk.spacing_m;
    let max = chunk.origin + (change.max.as_vec2() + Vec2::ONE) * chunk.spacing_m;
    Some((min, max))
}

pub(super) fn cells_for_region(
    grid: TopologyGrid,
    min: Vec2,
    max: Vec2,
    reach: f32,
) -> (IVec2, IVec2) {
    let padding = Vec2::splat(reach);
    (
        ((min - padding - grid.origin) / grid.spacing_m)
            .floor()
            .as_ivec2(),
        ((max + padding - grid.origin) / grid.spacing_m)
            .ceil()
            .as_ivec2(),
    )
}

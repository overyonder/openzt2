use bevy::prelude::*;
use openzt2_game_data::ui_document::overview_map_presentation::UiMapColorsRecord;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::overview_map_pixel_operations::rasterize_world_sample_area_into_overview_map_pixels;

pub(super) fn rasterize_terrain_or_water_into_overview_map_pixels(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_assets: &Assets<TerrainAsset>,
    world_definitions: WorldDefinitionsView<'_>,
    rasterize_only_water: bool,
    rasterization_palette: &UiMapColorsRecord,
) {
    for (terrain_chunk, edited_terrain_samples) in terrain_chunks {
        let Some(terrain_asset) = terrain_assets.get(&terrain_chunk.asset) else {
            continue;
        };
        for sample_z_index in 0..usize::from(terrain_chunk.side) {
            for sample_x_index in 0..usize::from(terrain_chunk.side) {
                let world_sample_position = terrain_chunk.origin
                    + Vec2::new(
                        sample_x_index as f32 * terrain_chunk.spacing_m,
                        sample_z_index as f32 * terrain_chunk.spacing_m,
                    );
                if rasterize_only_water {
                    let authored_terrain_sample =
                        crate::plugins::terrain::terrain_world_sampling::sample_authored_terrain(
                            terrain_chunk,
                            terrain_asset,
                            edited_terrain_samples,
                            world_sample_position,
                        );
                    if authored_terrain_sample.is_some_and(|sample| sample.water.is_some()) {
                        rasterize_world_sample_area_into_overview_map_pixels(
                            overview_map_pixels,
                            world_bounds,
                            world_sample_position,
                            terrain_chunk.spacing_m,
                            rasterization_palette.water,
                        );
                    }
                    continue;
                }
                let biome_definition_id =
                    crate::plugins::terrain::terrain_world_sampling::sample_authored_terrain(
                        terrain_chunk,
                        terrain_asset,
                        edited_terrain_samples,
                        world_sample_position,
                    )
                    .and_then(|terrain_sample| {
                        terrain_sample
                            .biome
                            .map(|biome_assignment| biome_assignment.id)
                    });
                let terrain_pixel_color = biome_definition_id
                    .and_then(|biome_definition_id| {
                        world_definitions.find_biome(biome_definition_id)
                    })
                    .map_or(rasterization_palette.terrain, |biome_definition| {
                        biome_definition.overview_colors.covered_ground
                    });
                rasterize_world_sample_area_into_overview_map_pixels(
                    overview_map_pixels,
                    world_bounds,
                    world_sample_position,
                    terrain_chunk.spacing_m,
                    terrain_pixel_color,
                );
            }
        }
    }
}

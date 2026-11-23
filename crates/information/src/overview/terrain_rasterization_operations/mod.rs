use bevy::prelude::*;
use openzt2_game_data::ui_document::overview_map_presentation::UiOverviewCanvas;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_world_sampling::sample_authored_terrain,
};

use super::{
    canvas_hydration_operations::OverviewCanvasPolicy,
    map_rasterization_operations::{
        OverviewTerrainWaterRaster, PendingOverviewMapPixels, blank_overview_map_pixels,
    },
    overview_map_pixel_operations::rasterize_world_sample_area_into_overview_map_pixels,
};

pub(crate) fn rasterize_dirty_overview_terrain_and_water_canvases(
    mut canvases: Query<
        (Entity, &OverviewCanvasPolicy, &mut PendingOverviewMapPixels),
        With<OverviewTerrainWaterRaster>,
    >,
    bounds: Query<&WorldTerrainHorizontalBounds>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
) {
    let dirty = canvases
        .iter()
        .filter_map(|(id, canvas, paint)| {
            (!paint.painted).then_some((id, canvas.source, canvas.palette))
        })
        .collect::<Vec<_>>();
    if dirty.is_empty() {
        return;
    }
    let (Ok(bounds), Some(definitions)) = (bounds.single(), definitions.get(&world_assets)) else {
        return;
    };
    let size = bounds.max - bounds.min;
    if !size.is_finite() || size.min_element() <= 0.0 || chunks.is_empty() {
        return;
    }
    let Some(chunks) = chunks
        .iter()
        .map(|(chunk, edits)| Some((chunk, edits, terrain_assets.get(&chunk.asset)?)))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let mut outputs = dirty
        .into_iter()
        .map(|(id, source, palette)| (id, source, palette, blank_overview_map_pixels()))
        .collect::<Vec<_>>();
    let render_terrain = outputs
        .iter()
        .any(|(_, source, _, _)| *source == UiOverviewCanvas::Terrain);
    for (chunk, edits, terrain) in chunks {
        for z in 0..chunk.side {
            for x in 0..chunk.side {
                let position =
                    chunk.origin + Vec2::new(f32::from(x), f32::from(z)) * chunk.spacing_m;
                let sample = sample_authored_terrain(chunk, terrain, edits, position);
                let water = sample.as_ref().is_some_and(|sample| sample.water.is_some());
                let color = render_terrain
                    .then(|| {
                        sample
                            .and_then(|sample| sample.biome)
                            .and_then(|biome| definitions.find_biome(biome.id))
                            .map(|biome| biome.overview_colors.covered_ground)
                    })
                    .flatten();
                for (_, source, palette, pixels) in &mut outputs {
                    let color = match source {
                        UiOverviewCanvas::Terrain => color.unwrap_or(palette.terrain),
                        UiOverviewCanvas::Water if water => palette.water,
                        _ => continue,
                    };
                    rasterize_world_sample_area_into_overview_map_pixels(
                        pixels,
                        bounds,
                        position,
                        chunk.spacing_m,
                        color,
                    );
                }
            }
        }
    }
    for (id, _, _, pixels) in outputs {
        if let Ok((_, _, mut output)) = canvases.get_mut(id) {
            output.pixels = Some(pixels);
            output.painted = true;
        }
    }
}

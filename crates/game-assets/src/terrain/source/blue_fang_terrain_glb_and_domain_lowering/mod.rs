use crate::model_source::conversion_error::ConversionError;
// Lowering of parsed Blue Fang terrain into the grid and GLB mesh.

use openzt2_game_data::terrain::{
    TerrainBiome, TerrainGrid, TerrainPresentation, TerrainSample, TerrainUnits, TerrainWaterDepth,
    TerrainWaterfall,
};

use super::{
    blue_fang_terrain_presentation_source_lowering::AuthoredTerrainHeights,
    blue_fang_terrain_source_parsing::blue_fang_terrain_cell_size_metres,
    blue_fang_terrain_source_types::{ParsedBlueFangTerrain, ParsedBlueFangTerrainSourceData},
};

pub struct LoweredBlueFangTerrain {
    pub grid: TerrainGrid,
    pub glb: Vec<u8>,
}

/// Lowers one original terrain grid into an ordinary indexed GLB mesh.
/// `heights` are the authored BFTerrain values resolved from the map's XML
/// documents.
pub fn lower_blue_fang_terrain_to_domain_grid_and_glb(
    terrain: ParsedBlueFangTerrain,
    heights: AuthoredTerrainHeights,
    presentation: TerrainPresentation,
    biomes: Vec<TerrainBiome>,
    waterfall: Option<TerrainWaterfall>,
) -> anyhow::Result<LoweredBlueFangTerrain> {
    let cell_size_metres = blue_fang_terrain_cell_size_metres(terrain.0.header)?;
    lower_blue_fang_terrain_with_validated_units(
        terrain.0,
        cell_size_metres,
        heights,
        presentation,
        biomes,
        waterfall,
    )
    .map_err(Into::into)
}

fn lower_blue_fang_terrain_with_validated_units(
    terrain: ParsedBlueFangTerrainSourceData,
    cell_size_metres: f32,
    heights: AuthoredTerrainHeights,
    presentation: TerrainPresentation,
    biomes: Vec<TerrainBiome>,
    waterfall: Option<TerrainWaterfall>,
) -> Result<LoweredBlueFangTerrain, ConversionError> {
    let base_height_metres = heights.base_height_metres;
    if !cell_size_metres.is_finite()
        || cell_size_metres <= 0.0
        || !base_height_metres.is_finite()
        || !heights.maximum_height_metres.is_finite()
        || heights
            .minimum_water_height_metres
            .is_some_and(|height| !height.is_finite())
    {
        return Err(ConversionError::InvalidValue(
            "invalid terrain unit contract",
        ));
    }
    if terrain.biome_names.len() != biomes.len()
        || terrain
            .biome_names
            .iter()
            .zip(&biomes)
            .any(|(source, biome)| !source.eq_ignore_ascii_case(&biome.name))
    {
        return Err(ConversionError::InvalidValue(
            "terrain biome bindings do not match the DAT token table",
        ));
    }
    if biomes.len() > 16
        || terrain.samples.iter().any(|sample| {
            sample.biome as usize >= biomes.len()
                || sample.ground_cover > 1
                || sample.water_type > 2
        })
    {
        return Err(ConversionError::InvalidValue(
            "terrain sample references an invalid biome, cover, or water type",
        ));
    }
    let width = usize::try_from(terrain.header.width)
        .map_err(|_| ConversionError::InvalidSource("terrain width exceeds usize"))?;
    let height = usize::try_from(terrain.header.height)
        .map_err(|_| ConversionError::InvalidSource("terrain height exceeds usize"))?;
    if terrain.header.sector_columns == 0
        || terrain.header.sector_rows == 0
        || (width - 1) % terrain.header.sector_columns as usize != 0
        || (height - 1) % terrain.header.sector_rows as usize != 0
    {
        return Err(ConversionError::InvalidSource(
            "terrain grid does not divide into authored sectors",
        ));
    }

    let grid = TerrainGrid {
        width: terrain.header.width,
        height: terrain.header.height,
        sector_columns: terrain.header.sector_columns,
        sector_rows: terrain.header.sector_rows,
        units: TerrainUnits {
            cell_size_metres,
            base_height_metres,
            maximum_height_metres: heights.maximum_height_metres,
            minimum_water_height_metres: heights.minimum_water_height_metres,
        },
        presentation,
        biomes,
        samples: terrain
            .samples
            .into_iter()
            .map(|sample| {
                let water_depth = match sample.water_type {
                    0 => TerrainWaterDepth::Dry,
                    1 => TerrainWaterDepth::Shallow,
                    2 => TerrainWaterDepth::Deep,
                    _ => {
                        return Err(ConversionError::InvalidValue(
                            "terrain sample has invalid water depth",
                        ));
                    }
                };
                Ok(TerrainSample {
                    height_relative_to_base_metres: sample.height,
                    biome_index: u8::try_from(sample.biome).map_err(|_| {
                        ConversionError::InvalidValue("terrain sample biome index exceeds u8")
                    })?,
                    has_ground_cover: sample.ground_cover != 0,
                    water_depth,
                    water_surface_flag: sample.water_surface_flag,
                    excludes_elevated_path: sample.excludes_elevated_path,
                    support_floor_metres: sample.minimum_height,
                    height_cap_metres: sample.maximum_height,
                    height_is_linked: sample.linked_height_shape.is_some_and(|value| value != 0),
                    has_child_height_link: sample
                        .constraint_flag
                        .is_some_and(|value| value & 1 != 0),
                })
            })
            .collect::<Result<Vec<_>, ConversionError>>()?,
        water_regions: terrain.water_regions,
        waterfall,
        slope_tiles: terrain.slope_tiles,
    };
    let glb =
        super::canonical_terrain_grid_mesh_construction::create_canonical_terrain_grid_glb(&grid)?;
    Ok(LoweredBlueFangTerrain { grid, glb })
}

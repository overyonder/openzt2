//! Binary parsing and source-layout validation for Blue Fang terrain DAT files.
use crate::assets::model_source::conversion_error::ConversionError;

use openzt2_game_data::terrain::{TerrainSlopeTile, TerrainWaterRegion};

use super::blue_fang_terrain_source_types::{
    BlueFangTerrainSourceHeader, ParsedBlueFangTerrain, ParsedBlueFangTerrainSourceData,
    ParsedBlueFangTerrainSourceSample,
};

const BLUE_FANG_TERRAIN_HEADER_BYTE_COUNT: usize = 44;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct BlueFangTerrainSourceSampleLayout {
    stride: usize,
    height: usize,
    relative_height: Option<usize>,
    biome: Option<usize>,
    cover: Option<usize>,
    water: Option<usize>,
    water_surface_flag: Option<usize>,
    excludes_elevated_path: Option<usize>,
    constraint_flag: Option<usize>,
    minimum_height: Option<usize>,
    maximum_height: Option<usize>,
    linked_height_shape: Option<usize>,
}

pub(in crate::assets) fn parse_blue_fang_terrain_source(
    source_bytes: &[u8],
) -> anyhow::Result<ParsedBlueFangTerrain> {
    parse_blue_fang_terrain_source_data(source_bytes)
        .map(ParsedBlueFangTerrain)
        .map_err(Into::into)
}

fn parse_blue_fang_terrain_source_data(
    source_bytes: &[u8],
) -> Result<ParsedBlueFangTerrainSourceData, ConversionError> {
    if source_bytes.len() < BLUE_FANG_TERRAIN_HEADER_BYTE_COUNT {
        return Err(ConversionError::InvalidSource("truncated terrain header"));
    }
    let header = parse_blue_fang_terrain_source_header(source_bytes)?;
    if header.width < 2 || header.height < 2 {
        return Err(ConversionError::InvalidSource(
            "terrain grid is smaller than one cell",
        ));
    }
    if (header.width - 1) % 4 != 0 || (header.height - 1) % 4 != 0 {
        return Err(ConversionError::InvalidSource(
            "terrain cells do not form complete four-cell slope tiles",
        ));
    }
    let layout = blue_fang_terrain_source_sample_layout(header.version)?;
    let mut source_byte_cursor = BLUE_FANG_TERRAIN_HEADER_BYTE_COUNT;
    let biome_names = parse_blue_fang_terrain_biome_names(source_bytes, &mut source_byte_cursor)?;
    let sample_count = usize::try_from(u64::from(header.width) * u64::from(header.height))
        .map_err(|_| ConversionError::InvalidSource("terrain sample count exceeds usize"))?;
    let record_bytes = layout
        .stride
        .checked_mul(sample_count)
        .and_then(|length| source_byte_cursor.checked_add(length))
        .filter(|end| *end <= source_bytes.len())
        .ok_or(ConversionError::InvalidSource("truncated terrain cells"))?;
    let samples = (0..sample_count)
        .map(|index| {
            let start = source_byte_cursor + index * layout.stride;
            Ok(ParsedBlueFangTerrainSourceSample {
                height: read_blue_fang_terrain_f32_at_offset(source_bytes, start + layout.height)?,
                relative_height_offset: layout
                    .relative_height
                    .map(|offset| {
                        read_blue_fang_terrain_f32_at_offset(source_bytes, start + offset)
                    })
                    .transpose()?,
                biome: layout.biome.map_or(Ok(0), |offset| {
                    read_blue_fang_terrain_u32_at_offset(source_bytes, start + offset)
                })?,
                ground_cover: layout
                    .cover
                    .map_or(0, |offset| source_bytes[start + offset]),
                water_type: layout.water.map_or(Ok(0), |offset| {
                    read_blue_fang_terrain_u32_at_offset(source_bytes, start + offset)
                })?,
                water_surface_flag: layout
                    .water_surface_flag
                    .is_some_and(|offset| source_bytes[start + offset] & 1 != 0),
                excludes_elevated_path: layout
                    .excludes_elevated_path
                    .is_some_and(|offset| source_bytes[start + offset] & 1 != 0),
                constraint_flag: layout
                    .constraint_flag
                    .map(|offset| source_bytes[start + offset]),
                minimum_height: layout
                    .minimum_height
                    .map(|offset| {
                        read_blue_fang_terrain_f32_at_offset(source_bytes, start + offset)
                    })
                    .transpose()?,
                maximum_height: layout
                    .maximum_height
                    .map(|offset| {
                        read_blue_fang_terrain_f32_at_offset(source_bytes, start + offset)
                    })
                    .transpose()?,
                linked_height_shape: layout
                    .linked_height_shape
                    .map(|offset| source_bytes[start + offset]),
            })
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    if samples.iter().any(|sample| {
        !sample.height.is_finite()
            || sample
                .relative_height_offset
                .is_some_and(|value| !value.is_finite())
            || sample
                .minimum_height
                .is_some_and(|value| !value.is_finite())
            || sample
                .maximum_height
                .is_some_and(|value| !value.is_finite())
            || sample
                .minimum_height
                .zip(sample.maximum_height)
                .is_some_and(|(minimum, maximum)| minimum > maximum)
    }) {
        return Err(ConversionError::InvalidValue(
            "terrain contains invalid height or constraint semantics",
        ));
    }
    let post_grid = parse_blue_fang_terrain_post_grid(header, &source_bytes[record_bytes..])?;
    Ok(ParsedBlueFangTerrainSourceData {
        header,
        samples,
        slope_tiles: post_grid.slope_tiles,
        biome_names,
        water_regions: post_grid.water_regions,
    })
}

fn parse_blue_fang_terrain_biome_names(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<Vec<String>, ConversionError> {
    let biome_count = usize::try_from(read_next_blue_fang_terrain_u32(
        source_bytes,
        source_byte_cursor,
    )?)
    .map_err(|_| ConversionError::InvalidSource("terrain biome count exceeds usize"))?;
    (0..biome_count)
        .map(|_| {
            let length = usize::try_from(read_next_blue_fang_terrain_u32(
                source_bytes,
                source_byte_cursor,
            )?)
            .map_err(|_| ConversionError::InvalidSource("terrain biome token exceeds usize"))?;
            let token =
                take_blue_fang_terrain_source_bytes(source_bytes, source_byte_cursor, length)?;
            String::from_utf8(token.strip_suffix(&[0]).unwrap_or(token).to_vec())
                .map_err(|_| ConversionError::InvalidSource("terrain biome token is not UTF-8"))
        })
        .collect()
}

fn parse_blue_fang_terrain_source_header(
    source_bytes: &[u8],
) -> Result<BlueFangTerrainSourceHeader, ConversionError> {
    if source_bytes.len() < BLUE_FANG_TERRAIN_HEADER_BYTE_COUNT {
        return Err(ConversionError::InvalidSource("truncated terrain header"));
    }
    Ok(BlueFangTerrainSourceHeader {
        version: read_blue_fang_terrain_u32_at_offset(source_bytes, 0)?,
        extent_x: read_blue_fang_terrain_f32_at_offset(source_bytes, 4)?,
        extent_y: read_blue_fang_terrain_f32_at_offset(source_bytes, 8)?,
        width: read_blue_fang_terrain_u32_at_offset(source_bytes, 28)?,
        height: read_blue_fang_terrain_u32_at_offset(source_bytes, 32)?,
        sector_columns: read_blue_fang_terrain_u32_at_offset(source_bytes, 36)?,
        sector_rows: read_blue_fang_terrain_u32_at_offset(source_bytes, 40)?,
    })
}

pub(super) fn blue_fang_terrain_cell_size_metres(
    header: BlueFangTerrainSourceHeader,
) -> Result<f32, ConversionError> {
    let cells_x = header
        .width
        .checked_sub(1)
        .ok_or(ConversionError::InvalidSource(
            "terrain grid has no horizontal cells",
        ))?;
    let cells_y = header
        .height
        .checked_sub(1)
        .ok_or(ConversionError::InvalidSource(
            "terrain grid has no vertical cells",
        ))?;
    let x = header.extent_x / cells_x as f32;
    let y = header.extent_y / cells_y as f32;
    (x.is_finite() && y.is_finite() && x > 0.0 && (x - y).abs() <= 1.0e-4)
        .then_some(x)
        .ok_or(ConversionError::InvalidValue(
            "terrain extents do not define isotropic cells",
        ))
}

fn blue_fang_terrain_source_sample_layout(
    source_version: u32,
) -> Result<BlueFangTerrainSourceSampleLayout, ConversionError> {
    if source_version > 14 {
        return Err(ConversionError::InvalidSource(
            "unsupported terrain version",
        ));
    }
    let mut stride = 4;
    let relative_height = (source_version >= 8).then(|| {
        let offset = stride;
        stride += 4;
        offset
    });
    let (biome, cover, water) = if source_version > 0 {
        let biome = stride;
        stride += 4;
        let cover = stride;
        stride += 1;
        let water = stride;
        stride += 4;
        (Some(biome), Some(cover), Some(water))
    } else {
        (None, None, None)
    };
    let (constraint_flag, minimum_height, maximum_height) = if source_version >= 6 {
        let flag = stride;
        stride += 1;
        let minimum = stride;
        stride += 4;
        let maximum = stride;
        stride += 4;
        (Some(flag), Some(minimum), Some(maximum))
    } else {
        (None, None, None)
    };
    let linked_height_shape = (source_version >= 9).then(|| {
        let offset = stride;
        stride += 1;
        offset
    });
    let excludes_elevated_path = (source_version >= 11).then(|| {
        let offset = stride;
        stride += 1;
        offset
    });
    let water_surface_flag = (source_version >= 13).then(|| {
        let offset = stride;
        stride += 1;
        offset
    });
    Ok(BlueFangTerrainSourceSampleLayout {
        stride,
        height: 0,
        relative_height,
        biome,
        cover,
        water,
        water_surface_flag,
        excludes_elevated_path,
        constraint_flag,
        minimum_height,
        maximum_height,
        linked_height_shape,
    })
}

struct ParsedBlueFangTerrainPostGrid {
    slope_tiles: Vec<TerrainSlopeTile>,
    water_regions: Vec<TerrainWaterRegion>,
}

fn parse_blue_fang_terrain_post_grid(
    header: BlueFangTerrainSourceHeader,
    source_bytes: &[u8],
) -> Result<ParsedBlueFangTerrainPostGrid, ConversionError> {
    let mut source_byte_cursor = 0;
    let water_count = read_next_blue_fang_terrain_u32(source_bytes, &mut source_byte_cursor)?;
    let mut water_regions = Vec::with_capacity(water_count as usize);
    for _ in 0..water_count {
        let height_relative_to_base_metres = f32::from_bits(read_next_blue_fang_terrain_u32(
            source_bytes,
            &mut source_byte_cursor,
        )?);
        let water_style_index =
            read_next_blue_fang_terrain_u32(source_bytes, &mut source_byte_cursor)?;
        let cell_count = usize::try_from(read_next_blue_fang_terrain_u32(
            source_bytes,
            &mut source_byte_cursor,
        )?)
        .map_err(|_| ConversionError::InvalidSource("terrain water cell count exceeds usize"))?;
        let row_major_cell_indices = (0..cell_count)
            .map(|_| read_next_blue_fang_terrain_u32(source_bytes, &mut source_byte_cursor))
            .collect::<Result<Vec<_>, _>>()?;
        let (depth_ratio, uses_tank_minimum_depth) = match header.version {
            10 | 11 => (0.0, false),
            13 => {
                let ratio = f32::from_bits(read_next_blue_fang_terrain_u32(
                    source_bytes,
                    &mut source_byte_cursor,
                )?);
                let source_flag_carrier =
                    read_next_blue_fang_terrain_u32(source_bytes, &mut source_byte_cursor)?;
                (ratio, source_flag_carrier & 0xff != 0)
            }
            14 => {
                let ratio = f32::from_bits(read_next_blue_fang_terrain_u32(
                    source_bytes,
                    &mut source_byte_cursor,
                )?);
                let source_flag =
                    take_blue_fang_terrain_source_bytes(source_bytes, &mut source_byte_cursor, 1)?
                        [0];
                (ratio, source_flag != 0)
            }
            _ => {
                return Err(ConversionError::InvalidSource(
                    "unsupported terrain post-grid layout",
                ));
            }
        };
        if !height_relative_to_base_metres.is_finite() || !depth_ratio.is_finite() {
            return Err(ConversionError::InvalidValue(
                "terrain water contains a non-finite scalar",
            ));
        }
        water_regions.push(TerrainWaterRegion {
            height_relative_to_base_metres,
            water_style_index,
            row_major_cell_indices,
            depth_ratio,
            uses_tank_minimum_depth,
        });
    }
    let count =
        usize::try_from(u64::from((header.width - 1) / 4) * u64::from((header.height - 1) / 4))
            .map_err(|_| {
                ConversionError::InvalidSource("terrain slope tile count exceeds usize")
            })?;
    let stride = if header.version == 10 { 1 } else { 2 };
    let tiles = (0..count)
        .map(|_| {
            let slope_tile_source_bytes =
                take_blue_fang_terrain_source_bytes(source_bytes, &mut source_byte_cursor, stride)?;
            let orientation = slope_tile_source_bytes[0];
            (orientation <= 1)
                .then_some(TerrainSlopeTile {
                    uses_alternate_diagonal: orientation != 0,
                })
                .ok_or(ConversionError::InvalidSource(
                    "invalid terrain slope orientation",
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if source_byte_cursor != source_bytes.len() {
        return Err(ConversionError::InvalidSource(
            "unowned terrain post-grid bytes",
        ));
    }
    Ok(ParsedBlueFangTerrainPostGrid {
        slope_tiles: tiles,
        water_regions,
    })
}

fn take_blue_fang_terrain_source_bytes<'a>(
    source_bytes: &'a [u8],
    source_byte_cursor: &mut usize,
    byte_count: usize,
) -> Result<&'a [u8], ConversionError> {
    let source_byte_range_end = source_byte_cursor
        .checked_add(byte_count)
        .ok_or(ConversionError::InvalidSource("terrain cursor overflow"))?;
    let result = source_bytes
        .get(*source_byte_cursor..source_byte_range_end)
        .ok_or(ConversionError::InvalidSource("truncated terrain file"))?;
    *source_byte_cursor = source_byte_range_end;
    Ok(result)
}

fn read_next_blue_fang_terrain_u32(
    source_bytes: &[u8],
    source_byte_cursor: &mut usize,
) -> Result<u32, ConversionError> {
    take_blue_fang_terrain_source_bytes(source_bytes, source_byte_cursor, 4)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| ConversionError::InvalidSource("invalid terrain u32 width"))
}

fn read_blue_fang_terrain_u32_at_offset(
    source_bytes: &[u8],
    source_byte_offset: usize,
) -> Result<u32, ConversionError> {
    source_bytes
        .get(source_byte_offset..source_byte_offset + 4)
        .ok_or(ConversionError::InvalidSource("truncated terrain scalar"))
        .and_then(|source_bytes| {
            source_bytes
                .try_into()
                .map(u32::from_le_bytes)
                .map_err(|_| ConversionError::InvalidSource("invalid terrain u32 width"))
        })
}

fn read_blue_fang_terrain_f32_at_offset(
    source_bytes: &[u8],
    source_byte_offset: usize,
) -> Result<f32, ConversionError> {
    read_blue_fang_terrain_u32_at_offset(source_bytes, source_byte_offset).map(f32::from_bits)
}

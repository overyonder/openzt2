//! Triangle indexing and height-field normals for Blue Fang terrain chunks.
use crate::assets::model_source::conversion_error::ConversionError;

use bevy::math::Vec3;

use super::blue_fang_terrain_source_types::ParsedBlueFangTerrainSourceData;

pub(super) fn terrain_chunk_triangle_indices(
    terrain: &ParsedBlueFangTerrainSourceData,
    width: usize,
    origin_x: usize,
    origin_z: usize,
    cells_x: usize,
    cells_z: usize,
) -> Result<Vec<u32>, ConversionError> {
    let tile_columns = (width - 1) / 4;
    let vertices_per_row = cells_x
        .checked_add(1)
        .ok_or(ConversionError::InvalidSource(
            "terrain chunk side exceeds usize",
        ))?;
    let gltf_vertices_per_row = u32::try_from(vertices_per_row)
        .map_err(|_| ConversionError::InvalidSource("terrain chunk side exceeds u32"))?;
    Ok((0..cells_z)
        .flat_map(|z| (0..cells_x).map(move |x| (x, z)))
        .map(|(x, z)| {
            let top_left = z
                .checked_mul(vertices_per_row)
                .and_then(|row_start| row_start.checked_add(x))
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(ConversionError::InvalidSource(
                    "terrain chunk vertex index exceeds u32",
                ))?;
            let top_right = top_left
                .checked_add(1)
                .ok_or(ConversionError::InvalidSource(
                    "terrain chunk vertex index exceeds u32",
                ))?;
            let bottom_left = top_left.checked_add(gltf_vertices_per_row).ok_or(
                ConversionError::InvalidSource("terrain chunk vertex index exceeds u32"),
            )?;
            let bottom_right = bottom_left
                .checked_add(1)
                .ok_or(ConversionError::InvalidSource(
                    "terrain chunk vertex index exceeds u32",
                ))?;
            let source_z = origin_z + cells_z - z - 1;
            let alternate = !terrain.slope_tiles[source_z / 4 * tile_columns + (origin_x + x) / 4]
                .uses_alternate_diagonal;
            Ok::<_, ConversionError>(if alternate {
                [
                    top_left,
                    bottom_left,
                    top_right,
                    top_right,
                    bottom_left,
                    bottom_right,
                ]
            } else {
                [
                    top_left,
                    bottom_left,
                    bottom_right,
                    top_left,
                    bottom_right,
                    top_right,
                ]
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect())
}

pub(super) fn terrain_height_field_normals(
    positions: &[[f32; 3]],
    width: usize,
    height: usize,
    cell_size_metres: f32,
) -> Vec<[f32; 3]> {
    (0..height)
        .flat_map(|z| {
            (0..width).map(move |x| {
                let left = height_at(positions, width, x.saturating_sub(1), z);
                let right = height_at(positions, width, (x + 1).min(width - 1), z);
                let top = height_at(positions, width, x, z.saturating_sub(1));
                let bottom = height_at(positions, width, x, (z + 1).min(height - 1));
                Vec3::new(left - right, 2.0 * cell_size_metres, top - bottom)
                    .normalize()
                    .to_array()
            })
        })
        .collect()
}

fn height_at(positions: &[[f32; 3]], width: usize, x: usize, z: usize) -> f32 {
    positions[z * width + x][1]
}

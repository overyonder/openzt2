//! Indexed Bevy meshes for the canonical terrain grid.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use openzt2_game_data::terrain::{TerrainGrid, TerrainWaterDepth};

use super::terrain_chunk_mesh_geometry::{
    terrain_chunk_triangle_indices, terrain_height_field_normals,
};
use crate::model::custom_vertex_attributes;
use crate::model_source::conversion_error::ConversionError;

pub(crate) fn construct_canonical_terrain_chunk_meshes(
    terrain: &TerrainGrid,
) -> Result<Vec<Mesh>, ConversionError> {
    let width = terrain.width as usize;
    let height = terrain.height as usize;
    let cell_size_metres = terrain.units.cell_size_metres;
    let base_height_metres = terrain.units.base_height_metres;
    let presentation = &terrain.presentation;
    if width < 2
        || height < 2
        || terrain.sector_columns == 0
        || terrain.sector_rows == 0
        || (width - 1) % terrain.sector_columns as usize != 0
        || (height - 1) % terrain.sector_rows as usize != 0
        || (width - 1) % 4 != 0
        || (height - 1) % 4 != 0
        || width.checked_mul(height) != Some(terrain.samples.len())
        || ((width - 1) / 4).checked_mul((height - 1) / 4) != Some(terrain.slope_tiles.len())
        || !cell_size_metres.is_finite()
        || cell_size_metres <= 0.0
        || !base_height_metres.is_finite()
        || terrain.samples.iter().any(|sample| {
            !sample.height_relative_to_base_metres.is_finite()
                || sample.biome_index as usize >= terrain.biomes.len()
        })
    {
        return Err(ConversionError::InvalidValue(
            "invalid terrain geometry grid",
        ));
    }
    let chunk_width = (width - 1) / terrain.sector_columns as usize;
    let chunk_height = (height - 1) / terrain.sector_rows as usize;
    let margin = f32::from(presentation.surface_margin_cells);
    let composed_width = chunk_width as f32 + margin * 2.0;
    let composed_height = chunk_height as f32 + margin * 2.0;
    let detail_scale = cell_size_metres * presentation.detail_texture_repetition;
    let mut meshes = Vec::new();
    for chunk_z in 0..terrain.sector_rows as usize {
        for chunk_x in 0..terrain.sector_columns as usize {
            let origin_x = chunk_x * chunk_width;
            let origin_z = chunk_z * chunk_height;
            let count = (chunk_width + 1) * (chunk_height + 1);
            let mut positions = Vec::with_capacity(count);
            let mut uv = Vec::with_capacity(count);
            let mut detail_uv = Vec::with_capacity(count);
            let mut biomes = Vec::with_capacity(count);
            let mut cover = Vec::with_capacity(count);
            let mut water = Vec::with_capacity(count);
            for z in 0..=chunk_height {
                let source_z = origin_z + chunk_height - z;
                for x in 0..=chunk_width {
                    let sample = &terrain.samples[source_z * width + origin_x + x];
                    positions.push([
                        x as f32 * cell_size_metres,
                        sample.height_relative_to_base_metres + base_height_metres,
                        z as f32 * cell_size_metres,
                    ]);
                    uv.push([
                        (x as f32 + margin) / composed_width,
                        (z as f32 + margin) / composed_height,
                    ]);
                    detail_uv.push([
                        (origin_x + x) as f32 * detail_scale,
                        (origin_z + z) as f32 * detail_scale,
                    ]);
                    biomes.push(u32::from(sample.biome_index));
                    cover.push(u32::from(sample.has_ground_cover));
                    water.push(match sample.water_depth {
                        TerrainWaterDepth::Dry => 0,
                        TerrainWaterDepth::Shallow => 1,
                        TerrainWaterDepth::Deep => 2,
                    });
                }
            }
            let normals = terrain_height_field_normals(
                &positions,
                chunk_width + 1,
                chunk_height + 1,
                cell_size_metres,
            );
            let indices = terrain_chunk_triangle_indices(
                &terrain.slope_tiles,
                width,
                origin_x,
                origin_z,
                chunk_width,
                chunk_height,
            )?;
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, detail_uv);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0_f32; 4]; count]);
            mesh.insert_attribute(
                custom_vertex_attributes::ATTRIBUTE_BIOME_INDEX,
                VertexAttributeValues::Uint32(biomes),
            );
            mesh.insert_attribute(
                custom_vertex_attributes::ATTRIBUTE_GROUND_COVER,
                VertexAttributeValues::Uint32(cover),
            );
            mesh.insert_attribute(
                custom_vertex_attributes::ATTRIBUTE_WATER_TYPE,
                VertexAttributeValues::Uint32(water),
            );
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh);
        }
    }
    Ok(meshes)
}

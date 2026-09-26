use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_sample_grid_queries::{
        read_terrain_sample_height_centimetres, terrain_cell_uses_alternate_triangle_diagonal,
    },
    terrain_world_sampling::terrain_cell_averaged_normal,
};

pub(super) fn construct_terrain_chunk_mesh(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
) -> Option<Mesh> {
    let side = chunk.side as usize;
    let mut positions = Vec::with_capacity(side * side);
    let mut normals = Vec::with_capacity(side * side);
    let mut uvs = Vec::with_capacity(side * side);
    let mut detail_uvs = Vec::with_capacity(side * side);
    let mut diffuse_colours = Vec::with_capacity(side * side);
    for z in 0..side {
        for x in 0..side {
            let height = read_terrain_sample_height_centimetres(chunk, asset, edited, x, z)?;
            positions.push([
                x as f32 * chunk.spacing_m,
                f32::from(height) * 0.01,
                z as f32 * chunk.spacing_m,
            ]);
            normals.push(
                terrain_cell_averaged_normal(
                    chunk,
                    asset,
                    edited,
                    UVec2::new(x.min(side - 2) as u32, z.min(side - 2) as u32),
                )
                .unwrap_or(Vec3::Y)
                .to_array(),
            );
            let cells = (side - 1) as f32;
            let margin = f32::from(
                asset
                    .canonical_terrain_grid()
                    .presentation
                    .surface_margin_cells,
            );
            let composed_cells = cells + margin * 2.0;
            uvs.push([
                (x as f32 + margin) / composed_cells,
                (z as f32 + margin) / composed_cells,
            ]);
            let detail_repetition = asset
                .canonical_terrain_grid()
                .presentation
                .detail_texture_repetition;
            detail_uvs.push([
                (chunk.origin.x + x as f32 * chunk.spacing_m) * detail_repetition,
                (chunk.origin.y + z as f32 * chunk.spacing_m) * detail_repetition,
            ]);
            diffuse_colours.push([1.0_f32; 4]);
        }
    }
    let mut indices = Vec::with_capacity((side - 1) * (side - 1) * 6);
    for z in 0..side - 1 {
        for x in 0..side - 1 {
            let a = u32::try_from(z * side + x).ok()?;
            let b = a + 1;
            let c = a + u32::from(chunk.side);
            let d = c + 1;
            let alternate = terrain_cell_uses_alternate_triangle_diagonal(
                chunk,
                asset,
                UVec2::new(x as u32, z as u32),
            )?;
            if alternate {
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            } else {
                indices.extend_from_slice(&[a, c, d, a, d, b]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, detail_uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, diffuse_colours);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

use bevy::mesh::Indices;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::authored_terrain_sample_index;

use crate::terrain::terrain_chunk_mesh_construction::construct_terrain_chunk_mesh;

pub(super) fn construct_caustic_ground_mesh(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    terrain_tint: [f32; 4],
    mut owns_sample: impl FnMut(usize) -> bool,
    mut renders_tile: impl FnMut(UVec2) -> bool,
) -> Option<Mesh> {
    let side = usize::from(chunk.side);
    let tint = terrain_tint.map(|channel| {
        // Match the position/packed-colour terrain stream before interpolation.
        let packed = ((f64::from(channel) * 255.0 + 0.5) as i32 as u32).min(255);
        packed as f32 / 255.0
    });
    let mut colours = Vec::with_capacity(side * side);
    let mut any_owned = false;
    for z in 0..side {
        for x in 0..side {
            let owned = owns_sample(authored_terrain_sample_index(chunk, asset, x, z)?);
            any_owned |= owned;
            colours.push(if owned { tint } else { [0.0; 4] });
        }
    }
    if !any_owned {
        return None;
    }
    // The projection uses the ground triangles, including the authored diagonal;
    // the owner mask interpolates across their shared vertices.
    let mut mesh = construct_terrain_chunk_mesh(chunk, asset, edited)?;
    let Some(Indices::U32(indices)) = mesh.indices_mut() else {
        return None;
    };
    let cells = side.checked_sub(1)?;
    let grid_width = asset.canonical_terrain_grid().width;
    let mut selected = Vec::with_capacity(indices.len());
    for (cell, triangles) in indices.chunks_exact(6).enumerate() {
        let source =
            authored_terrain_sample_index(chunk, asset, cell % cells, cell / cells + 1)? as u32;
        let tile = UVec2::new(source % grid_width, source / grid_width) / 4;
        if renders_tile(tile) {
            selected.extend_from_slice(triangles);
        }
    }
    if selected.is_empty() {
        return None;
    }
    *indices = selected;
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    Some(mesh)
}

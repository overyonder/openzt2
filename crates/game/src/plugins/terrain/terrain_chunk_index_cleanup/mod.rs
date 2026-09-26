use bevy::prelude::*;

use super::terrain_chunk_types::{TerrainChunk, TerrainIndex};

pub(super) fn remove_despawned_terrain_chunks_from_spatial_index(
    mut removed_chunks: RemovedComponents<TerrainChunk>,
    mut terrain_index: ResMut<TerrainIndex>,
) {
    for removed_chunk_entity in removed_chunks.read() {
        terrain_index
            .chunks
            .retain(|_, indexed_chunk_entity| *indexed_chunk_entity != removed_chunk_entity);
    }
    if terrain_index.chunks.is_empty() {
        terrain_index.origin = Vec2::ZERO;
        terrain_index.chunk_span_m = 0.0;
    }
}

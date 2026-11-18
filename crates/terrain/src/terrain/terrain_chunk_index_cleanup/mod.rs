use std::collections::HashMap;

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};

use openzt2_zoo_state::terrain::{
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_presentation_types::TerrainRenderChunk,
    terrain_chunk_types::{TerrainChunk, TerrainIndex},
};

#[derive(Resource, Default)]
pub(super) struct TerrainChunkIdLookup(pub(super) HashMap<TerrainChunkId, Entity>);

pub(super) fn index_validated_terrain_chunk(
    added: On<Add<TerrainRenderChunk>>,
    chunks: Query<(&TerrainChunk, &TerrainChunkId)>,
    mut terrain: ResMut<TerrainIndex>,
    mut ids: ResMut<TerrainChunkIdLookup>,
) {
    let Ok((chunk, id)) = chunks.get(added.entity) else {
        return;
    };
    if terrain.chunks.is_empty() {
        terrain.chunk_span_m = f32::from(chunk.side - 1) * chunk.spacing_m;
        terrain.origin = chunk.origin - chunk.coord.as_vec2() * terrain.chunk_span_m;
    }
    terrain.chunks.insert(chunk.coord, added.entity);
    ids.0.insert(*id, added.entity);
}

pub(super) fn remove_discarded_terrain_chunk_from_indexes(
    discarded: On<Discard<TerrainChunk>>,
    chunks: Query<(&TerrainChunk, &TerrainChunkId), Allow<Disabled>>,
    mut terrain: ResMut<TerrainIndex>,
    mut ids: ResMut<TerrainChunkIdLookup>,
) {
    let Ok((chunk, id)) = chunks.get(discarded.entity) else {
        return;
    };
    if terrain.chunks.get(&chunk.coord) == Some(&discarded.entity) {
        terrain.chunks.remove(&chunk.coord);
        if terrain.chunks.is_empty() {
            terrain.origin = Vec2::ZERO;
            terrain.chunk_span_m = 0.0;
        }
    }
    if ids.0.get(id) == Some(&discarded.entity) {
        ids.0.remove(id);
    }
}

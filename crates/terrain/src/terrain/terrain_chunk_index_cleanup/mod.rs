use std::collections::HashMap;

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::terrain::{
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_presentation_types::TerrainRenderChunk,
    terrain_chunk_types::{TerrainChunk, TerrainIndex},
};

#[derive(Component, Default)]
pub(super) struct TerrainChunkIdLookup(pub(super) HashMap<TerrainChunkId, Entity>);

pub(super) fn index_validated_terrain_chunk(
    added: On<Insert<TerrainRenderChunk>>,
    chunks: Query<(&TerrainChunk, &TerrainChunkId, &WorldMember)>,
    mut roots: Query<(&mut TerrainIndex, &mut TerrainChunkIdLookup), Allow<Disabled>>,
) {
    let Ok((chunk, id, member)) = chunks.get(added.entity) else {
        return;
    };
    let Ok((mut terrain, mut ids)) = roots.get_mut(member.root) else {
        return;
    };
    if terrain
        .chunks
        .get(&chunk.coord)
        .is_some_and(|entity| *entity != added.entity)
        || ids.0.get(id).is_some_and(|entity| *entity != added.entity)
    {
        return;
    }
    if terrain.chunks.is_empty() {
        terrain.chunk_span_m = f32::from(chunk.side - 1) * chunk.spacing_m;
        terrain.origin = chunk.origin - chunk.coord.as_vec2() * terrain.chunk_span_m;
    }
    terrain.chunks.insert(chunk.coord, added.entity);
    ids.0.insert(*id, added.entity);
}

pub(super) fn remove_discarded_terrain_chunk_from_indexes(
    discarded: On<Discard<TerrainChunk>>,
    chunks: Query<(&TerrainChunk, &TerrainChunkId, &WorldMember), Allow<Disabled>>,
    mut roots: Query<(&mut TerrainIndex, &mut TerrainChunkIdLookup), Allow<Disabled>>,
) {
    let Ok((chunk, id, member)) = chunks.get(discarded.entity) else {
        return;
    };
    let Ok((mut terrain, mut ids)) = roots.get_mut(member.root) else {
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

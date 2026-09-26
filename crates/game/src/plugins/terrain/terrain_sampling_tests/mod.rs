use bevy::prelude::*;

use super::{terrain_chunk_types::TerrainIndex, terrain_world_sampling::terrain_chunk_at};

#[test]
fn terrain_chunk_lookup_uses_floor_for_negative_world_coordinates() {
    let negative_coordinate_chunk = Entity::from_bits(1 << 32 | 1);
    let nonnegative_coordinate_chunk = Entity::from_bits(1 << 32 | 2);
    let mut terrain_index = TerrainIndex {
        chunks: default(),
        origin: Vec2::ZERO,
        chunk_span_m: 10.0,
    };
    terrain_index
        .chunks
        .insert(IVec2::new(-1, 0), negative_coordinate_chunk);
    terrain_index
        .chunks
        .insert(IVec2::new(0, 0), nonnegative_coordinate_chunk);
    assert_eq!(
        terrain_chunk_at(&terrain_index, Vec2::new(-0.01, 2.0)),
        Some(negative_coordinate_chunk)
    );
    assert_eq!(
        terrain_chunk_at(&terrain_index, Vec2::new(0.0, 2.0)),
        Some(nonnegative_coordinate_chunk)
    );
    assert_eq!(
        terrain_chunk_at(&terrain_index, Vec2::splat(f32::NAN)),
        None
    );
}

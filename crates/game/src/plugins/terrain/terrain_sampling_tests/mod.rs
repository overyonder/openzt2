use bevy::prelude::*;

use super::{
    terrain_chunk_types::TerrainIndex,
    terrain_world_sampling::{minimum_terrain_triangle_normal_y, terrain_chunk_at},
};

#[test]
fn triangle_slope_bound_matches_mesh_faces_instead_of_averaging_them() {
    // The two saddle normals cancel horizontally, making their average flat.
    let corners = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 2.0, 0.0),
        Vec3::new(0.0, 2.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
    ];
    for (alternate, faces) in [
        (false, [[0, 2, 3], [0, 3, 1]]),
        (true, [[0, 2, 1], [1, 2, 3]]),
    ] {
        let normals = faces.map(|[a, b, c]| {
            (corners[b] - corners[a])
                .cross(corners[c] - corners[a])
                .normalize()
        });
        assert!((normals[0] + normals[1])
            .normalize()
            .abs_diff_eq(Vec3::Y, 1.0e-6));
        let bound = minimum_terrain_triangle_normal_y(corners.map(|point| point.y), 1.0, alternate)
            .unwrap();
        assert!((bound - normals[0].y.min(normals[1].y)).abs() < 1.0e-6);
        assert!(bound < 0.5);
    }
}

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

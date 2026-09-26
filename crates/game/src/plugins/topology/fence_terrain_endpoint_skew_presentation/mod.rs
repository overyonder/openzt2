//! Terrain endpoint fitting for the source-authored fence skew component.

use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;

use super::{
    topology_graph_types::TopologyGrid,
    topology_presentation_types::{
        FenceTerrainEndpointPresentationCells, FenceTerrainEndpointSkewPresentation,
    },
};

/// Fits each changed fence segment to the terrain heights at its two canonical
/// topology endpoints. Shipped fence meshes start at local X zero and carry
/// `BFSkewComponent axis="0"`, so the owner stays at the first endpoint while
/// two nested Bevy transforms shear local X toward the second endpoint.
#[allow(clippy::too_many_arguments)]
pub(super) fn fit_fence_presentations_to_terrain_endpoint_heights(
    mut terrain_changed: MessageReader<TerrainChanged>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    grid: Res<TopologyGrid>,
    mut fences: Query<(
        Ref<FenceTerrainEndpointPresentationCells>,
        Ref<FenceTerrainEndpointSkewPresentation>,
        &mut Transform,
    )>,
    mut presentation_transforms: Query<
        &mut Transform,
        Without<FenceTerrainEndpointPresentationCells>,
    >,
) {
    let terrain_was_changed = terrain_changed.read().next().is_some();
    for (endpoint_cells, presentation, mut fence_transform) in &mut fences {
        if !terrain_was_changed
            && !endpoint_cells.is_changed()
            && !presentation.is_changed()
            && !grid.is_changed()
        {
            continue;
        }
        let first_world_xz = grid.cell_translation(endpoint_cells.first).xz();
        let second_world_xz = grid.cell_translation(endpoint_cells.second).xz();
        let Some(first_height_m) = sample_terrain_height_at_world_position(
            first_world_xz,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
        ) else {
            continue;
        };
        let Some(second_height_m) = sample_terrain_height_at_world_position(
            second_world_xz,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
        ) else {
            continue;
        };
        let horizontal_length_m = first_world_xz.distance(second_world_xz);
        if !horizontal_length_m.is_finite() || horizontal_length_m <= f32::EPSILON {
            continue;
        }
        fence_transform.translation.y = first_height_m;
        let height_change_per_horizontal_metre =
            (second_height_m - first_height_m) / horizontal_length_m;
        let (first_transform, second_transform) =
            decompose_local_x_vertical_shear_into_two_bevy_transforms(
                height_change_per_horizontal_metre,
            );
        let Ok([mut first, mut second]) = presentation_transforms
            .get_many_mut([presentation.first_transform, presentation.second_transform])
        else {
            continue;
        };
        *first = first_transform;
        *second = second_transform;
    }
}

fn sample_terrain_height_at_world_position(
    world_xz: Vec2,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<f32> {
    let terrain_chunk = terrain_chunk_at(terrain_index, world_xz)?;
    let (chunk, edited) = terrain_chunks.get(terrain_chunk).ok()?;
    let terrain_asset = terrain_assets.get(&chunk.asset)?;
    sample_terrain(chunk, terrain_asset, edited, world_xz).map(|point| point.height_m)
}

/// A vertical shear along authored local X cannot be represented by one TRS
/// transform. Its 2D SVD is a rotation, non-uniform scale, and second rotation,
/// which maps directly to two nested Bevy `Transform` components while leaving
/// shared mesh assets untouched.
fn decompose_local_x_vertical_shear_into_two_bevy_transforms(
    height_change_per_horizontal_metre: f32,
) -> (Transform, Transform) {
    if height_change_per_horizontal_metre.abs() <= f32::EPSILON {
        return (Transform::IDENTITY, Transform::IDENTITY);
    }
    let shear = height_change_per_horizontal_metre;
    let largest_squared_scale =
        (2.0 + shear * shear + shear.abs() * (shear * shear + 4.0).sqrt()) * 0.5;
    let largest_scale = largest_squared_scale.sqrt();
    let smallest_scale = largest_scale.recip();
    let first_right_singular_vector = Vec2::new(shear, largest_squared_scale - 1.0).normalize();
    let second_right_singular_vector = Vec2::new(
        -first_right_singular_vector.y,
        first_right_singular_vector.x,
    );
    let apply_shear = |vector: Vec2| Vec2::new(vector.x + shear * vector.y, vector.y);
    let first_left_singular_vector = apply_shear(first_right_singular_vector) / largest_scale;
    let second_left_singular_vector = apply_shear(second_right_singular_vector) / smallest_scale;
    let left_rotation_angle = first_left_singular_vector
        .y
        .atan2(first_left_singular_vector.x);
    let right_rotation_angle = first_right_singular_vector
        .y
        .atan2(first_right_singular_vector.x);
    debug_assert!(first_left_singular_vector
        .perp_dot(second_left_singular_vector)
        .is_sign_positive());
    // The vectors above decompose [1 shear; 0 1]. The required local-X
    // vertical shear [1 0; shear 1] is its transpose, so exchange its left and
    // right rotations when embedding the result in Bevy's XY plane.
    (
        Transform::from_rotation(Quat::from_rotation_z(right_rotation_angle))
            .with_scale(Vec3::new(largest_scale, smallest_scale, 1.0)),
        Transform::from_rotation(Quat::from_rotation_z(-left_rotation_angle)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_bevy_transforms_reproduce_local_x_vertical_endpoint_shear() {
        for shear in [-1.7, -0.5, 0.0, 0.5, 1.7] {
            let (first, second) = decompose_local_x_vertical_shear_into_two_bevy_transforms(shear);
            let composed = first.compute_affine() * second.compute_affine();

            assert!(composed
                .transform_point3(Vec3::Y)
                .abs_diff_eq(Vec3::Y, 0.000_01));
            assert!(composed
                .transform_point3(Vec3::X)
                .abs_diff_eq(Vec3::new(1.0, shear, 0.0), 0.000_01));
        }
    }
}

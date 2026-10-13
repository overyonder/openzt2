//! Terrain endpoint fitting for the source-authored fence skew component.

use bevy::{diagnostic::FrameCount, prelude::*};

use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::verification::verification_capture_readiness::world_presentation_diagnostics::{
    FenceTerrainFittingAttempt, FenceTerrainFittingObservation,
};
use crate::verification::verification_staff_gate_topology_diagnostics::StaffGateTopologyObservation;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

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
    frame: Option<Res<FrameCount>>,
    mut observation: Option<ResMut<StaffGateTopologyObservation>>,
    mut fitting_history: FenceTerrainFittingObservation,
    mut fences: Query<(
        Entity,
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
    fitting_history.begin_frame();
    for (entity, endpoint_cells, presentation, mut fence_transform) in &mut fences {
        let triggers = [
            terrain_was_changed,
            endpoint_cells.is_changed(),
            presentation.is_changed(),
            grid.is_changed(),
        ];
        let gate_skipped = !triggers.iter().any(|changed| *changed);
        let observed_owner = fitting_history.observes_owner(entity);
        if gate_skipped && !observed_owner {
            continue;
        }
        let first_world_xz = grid.cell_translation(endpoint_cells.first).xz();
        let second_world_xz = grid.cell_translation(endpoint_cells.second).xz();
        let skew_before = if observed_owner {
            presentation
                .transform_entities()
                .map(|entity| presentation_transforms.get(entity).ok().copied())
        } else {
            [None, None]
        };
        let mut attempt = if observed_owner {
            fitting_history.begin_owner(
                entity,
                endpoint_cells.endpoint_cells(),
                presentation.transform_entities(),
                triggers,
                first_world_xz.distance(second_world_xz),
                *fence_transform,
                skew_before,
            )
        } else {
            None
        };
        let frame_number = frame.as_ref().map(|frame| frame.0);
        if gate_skipped {
            // Only the four observed owners probe the same sampler with the
            // production gate closed. No transforms or fitting triggers change.
            for (endpoint, world_xz) in [first_world_xz, second_world_xz].into_iter().enumerate() {
                let _ = sample_terrain_height_at_world_position(
                    world_xz,
                    &terrain_index,
                    &terrain_assets,
                    &terrain_chunks,
                    attempt
                        .as_mut()
                        .map(|attempt| (&fitting_history, attempt, endpoint)),
                );
            }
            fitting_history.finish_owner(
                attempt,
                "gate skipped; observation-only samples; no production fit",
                *fence_transform,
                skew_before,
                frame_number,
            );
            continue;
        }
        if let Some(observation) = observation.as_mut() {
            observation.record_endpoint_fitting(
                entity,
                &endpoint_cells,
                *grid,
                frame.as_ref().map(|frame| frame.0),
                None,
                "fitting attempted but not completed",
            );
        }
        let Some(first_height_m) = sample_terrain_height_at_world_position(
            first_world_xz,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
            attempt
                .as_mut()
                .map(|attempt| (&fitting_history, attempt, 0)),
        ) else {
            fitting_history.finish_owner(
                attempt,
                "first endpoint sample unavailable",
                *fence_transform,
                skew_before,
                frame_number,
            );
            continue;
        };
        let Some(second_height_m) = sample_terrain_height_at_world_position(
            second_world_xz,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
            attempt
                .as_mut()
                .map(|attempt| (&fitting_history, attempt, 1)),
        ) else {
            fitting_history.finish_owner(
                attempt,
                "second endpoint sample unavailable",
                *fence_transform,
                skew_before,
                frame_number,
            );
            continue;
        };
        let horizontal_length_m = first_world_xz.distance(second_world_xz);
        if !horizontal_length_m.is_finite() || horizontal_length_m <= f32::EPSILON {
            fitting_history.finish_owner(
                attempt,
                "horizontal length rejected",
                *fence_transform,
                skew_before,
                frame_number,
            );
            continue;
        }
        fence_transform.translation.y = first_height_m;
        let height_change_per_horizontal_metre =
            (second_height_m - first_height_m) / horizontal_length_m;
        let (first_transform, second_transform) =
            decompose_local_x_vertical_shear_into_two_bevy_transforms(
                height_change_per_horizontal_metre,
            );
        let child_transforms = presentation_transforms
            .get_many_mut([presentation.first_transform, presentation.second_transform]);
        if let Some(attempt) = attempt.as_mut() {
            attempt.record_child_query_result(
                child_transforms
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| *error),
            );
        }
        let Ok([mut first, mut second]) = child_transforms else {
            fitting_history.finish_owner(
                attempt,
                "child query failed after owner height written",
                *fence_transform,
                skew_before,
                frame_number,
            );
            continue;
        };
        *first = first_transform;
        *second = second_transform;
        fitting_history.finish_owner(
            attempt,
            "production fit completed",
            *fence_transform,
            [Some(*first), Some(*second)],
            frame_number,
        );
        if let Some(observation) = observation.as_mut() {
            observation.record_endpoint_fitting(
                entity,
                &endpoint_cells,
                *grid,
                frame.as_ref().map(|frame| frame.0),
                Some((
                    Vec3::new(first_world_xz.x, first_height_m, first_world_xz.y),
                    Vec3::new(second_world_xz.x, second_height_m, second_world_xz.y),
                )),
                "existing terrain endpoint fitting completed",
            );
        }
    }
}

fn sample_terrain_height_at_world_position(
    world_xz: Vec2,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    observation: Option<(
        &FenceTerrainFittingObservation,
        &mut FenceTerrainFittingAttempt,
        usize,
    )>,
) -> Option<f32> {
    let terrain_chunk = terrain_chunk_at(terrain_index, world_xz);
    let chunk = terrain_chunk.and_then(|entity| terrain_chunks.get(entity).ok());
    let terrain_asset = chunk.and_then(|(chunk, _)| terrain_assets.get(&chunk.asset));
    let height_m = chunk
        .zip(terrain_asset)
        .and_then(|((chunk, edited), asset)| {
            sample_terrain(chunk, asset, edited, world_xz).map(|point| point.height_m)
        });
    if let Some((observation, attempt, endpoint)) = observation {
        observation.record_endpoint_sample(
            attempt,
            endpoint,
            world_xz,
            terrain_index,
            terrain_chunk,
            chunk,
            terrain_asset.is_some(),
            height_m,
        );
    }
    height_m
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

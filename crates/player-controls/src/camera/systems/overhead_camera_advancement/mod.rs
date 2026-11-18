use avian3d::prelude::{
    Collider, ColliderAabb, ColliderDisabled, Position, Rotation, SpatialQuery,
};
use bevy::{camera::ScalingMode, prelude::*};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::advance_pan_accumulators;
use super::super::math::apply_camera_relative_pan_to_overhead_focus;
use super::super::math::apply_overhead_zoom_delta;
use super::super::math::approach_scalar;
use super::super::math::clamp_rig;
use super::super::math::consume_wheel_zoom;
use super::super::math::overhead_eye_and_target;
use super::super::math::overhead_pose;
use super::camera_ground_fit_surface_sampling::{
    sample_highest_camera_fitting_collider,
    sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood,
};
use super::camera_pose_comparison::camera_pose_materially_changed;
use super::overhead_camera_zoom_policy::overhead_camera_zoom_speed_multiplier;
use crate::camera::camera_execution_types::CameraBounds;
use crate::camera::camera_execution_types::CameraIntent;
use crate::camera::camera_execution_types::CameraTransition;
use crate::camera::camera_execution_types::CameraTuning;

#[derive(Component, Default)]
pub(crate) struct CameraFittingSurfaceSample {
    probe_center: Option<Vec2>,
    terrain_height: Option<f32>,
    collision_height_and_entity: Option<(f32, Entity)>,
}

pub(crate) fn advance_overhead_camera(
    time: Res<Time<Real>>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    spatial_query: SpatialQuery,
    changed_terrain: Query<(), Or<(Changed<TerrainChunk>, Changed<EditedTerrainSamples>)>>,
    mut removed_edits: RemovedComponents<EditedTerrainSamples>,
    active_colliders: Query<&ColliderAabb, (With<Collider>, Without<ColliderDisabled>)>,
    changed_colliders: Query<
        (Entity, &ColliderAabb),
        (
            With<Collider>,
            Or<(
                Changed<ColliderAabb>,
                Changed<Collider>,
                Changed<Position>,
                Changed<Rotation>,
                Added<ColliderDisabled>,
            )>,
        ),
    >,
    mut enabled_colliders: RemovedComponents<ColliderDisabled>,
    mut cameras: Query<
        (
            &mut Transform,
            &mut Projection,
            &mut OverheadRig,
            &CameraTuning,
            &CameraBounds,
            &mut CameraIntent,
            &CameraMode,
            &mut CameraFittingSurfaceSample,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let dt = time.delta_secs();
    let terrain_edits_removed = removed_edits.read().count() != 0;
    let terrain_changed = terrain_index.is_changed()
        || terrain_assets.is_changed()
        || !changed_terrain.is_empty()
        || terrain_edits_removed;
    let reenabled_colliders = enabled_colliders.read().collect::<Vec<_>>();
    for (mut transform, mut projection, mut rig, tuning, bounds, mut intent, mode, mut sample) in
        &mut cameras
    {
        if *mode != CameraMode::Overhead || !dt.is_finite() || dt <= 0.0 {
            continue;
        }

        let speed_modifier =
            overhead_camera_zoom_speed_multiplier(rig.distance, rig.minimum_zoom_offset_m, tuning);
        let pan_axis = advance_pan_accumulators(
            &mut rig.pan_accumulators,
            intent.pan_directions,
            tuning.pan_start_rate,
            tuning.pan_stop_rate,
            dt,
        );
        let wheel_zoom = consume_wheel_zoom(&mut intent.wheel_zoom, dt);

        let pan_delta = pan_axis * tuning.pan_speed_mps * speed_modifier * dt;
        apply_camera_relative_pan_to_overhead_focus(&mut rig, pan_delta);
        rig.yaw += intent.turn * tuning.turn_speed_rps * dt;
        rig.pitch += intent.pitch * tuning.turn_speed_rps * dt;
        apply_overhead_zoom_delta(
            &mut rig,
            tuning,
            bounds.camera_clearance_m,
            -(intent.zoom + wheel_zoom) * tuning.zoom_speed_mps * speed_modifier * dt,
        );
        clamp_rig(&mut rig, tuning, bounds);
        if let Projection::Orthographic(current) = &*projection {
            let desired_height = rig.distance * tuning.vertical_view_per_zoom;
            if !matches!(&current.scaling_mode, ScalingMode::FixedVertical { viewport_height }
                if *viewport_height == desired_height)
            {
                if let Projection::Orthographic(projection) = &mut *projection {
                    projection.scaling_mode = ScalingMode::FixedVertical {
                        viewport_height: desired_height,
                    };
                }
            }
        }

        // The native overhead hierarchy probes a 3x3 neighbourhood around the
        // camera node against every active collision/selector fitting surface.
        // It applies the softened residual to that node and, when authored
        // `noTiltOnFit` is enabled, applies the same residual above the target
        // node's authored base height. Keep those node translations explicit
        // rather than replacing the saved hierarchy root.
        let mut unfit_rig = rig.clone();
        unfit_rig.camera_ground_fit_offset_m = 0.0;
        unfit_rig.target_ground_fit_offset_m = 0.0;
        let (unfit_eye, _) = overhead_eye_and_target(&unfit_rig);
        let probe = unfit_eye.xz();
        let probe_moved = sample.probe_center != Some(probe);
        if probe_moved || terrain_changed {
            sample.terrain_height =
                sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
                    probe, &terrain_index, &terrain_assets, &terrain_chunks,
                );
        }
        let previous_surface = sample.collision_height_and_entity.map(|(_, entity)| entity);
        let overlaps_probes = |aabb: &ColliderAabb| {
            let minimum = probe - Vec2::ONE;
            let maximum = probe + Vec2::ONE;
            aabb.min.x <= maximum.x
                && aabb.max.x >= minimum.x
                && aabb.min.z <= maximum.y
                && aabb.max.z >= minimum.y
        };
        if probe_moved
            || previous_surface.is_some_and(|entity| !active_colliders.contains(entity))
            || changed_colliders
                .iter()
                .any(|(entity, aabb)| Some(entity) == previous_surface || overlaps_probes(aabb))
            || reenabled_colliders
                .iter()
                .any(|entity| active_colliders.get(*entity).is_ok_and(overlaps_probes))
        {
            sample.collision_height_and_entity =
                sample_highest_camera_fitting_collider(probe, &spatial_query);
        }
        if probe_moved {
            sample.probe_center = Some(probe);
        }
        let required_camera_offset = sample
            .terrain_height
            .into_iter()
            .chain(sample.collision_height_and_entity.map(|(height, _)| height))
            .reduce(f32::max)
            .map(|height| {
                let ground_buffer_m = bounds.camera_clearance_m - tuning.collision_radius_m;
                (height + ground_buffer_m - unfit_eye.y).max(0.0)
            });
        if let Some(required_offset) = required_camera_offset {
            rig.camera_ground_fit_offset_m = advance_authored_overhead_camera_node_soft_fit(
                rig.camera_ground_fit_offset_m,
                required_offset,
                tuning,
                dt,
            );
        }
        rig.target_ground_fit_offset_m = tuning
            .no_tilt_on_fit
            .then_some(rig.camera_ground_fit_offset_m)
            .unwrap_or(0.0);
        // Pan, rotation and zoom still apply when no terrain sample is available.
        let next = overhead_pose(&rig);
        if camera_pose_materially_changed(&transform, &next) {
            debug!(
                target: "openzt2_camera_fitting",
                delta_seconds = dt,
                ?pan_axis,
                ?required_camera_offset,
                ?unfit_eye,
                rig = ?*rig,
                eye = ?next.translation,
                "overhead camera pose changed"
            );
            *transform = next;
        }
    }
}

fn advance_authored_overhead_camera_node_soft_fit(
    current_offset_m: f32,
    required_offset_m: f32,
    tuning: &CameraTuning,
    delta_seconds: f32,
) -> f32 {
    let next_offset_m = if tuning.soft_fit_speed_mps > 0.0 {
        approach_scalar(
            current_offset_m,
            required_offset_m,
            tuning.soft_fit_speed_mps,
            delta_seconds,
        )
    } else {
        required_offset_m
    };
    if (next_offset_m - required_offset_m).abs() <= tuning.soft_fit_distance_m {
        required_offset_m
    } else {
        next_offset_m
    }
}

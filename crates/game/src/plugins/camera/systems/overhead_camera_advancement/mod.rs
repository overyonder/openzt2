use avian3d::prelude::SpatialQuery;
use bevy::{camera::ScalingMode, prelude::*};

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
};

use super::{
    super::{
        camera_runtime_state_types::{
            CameraBounds, CameraIntent, CameraMode, CameraTransition, CameraTuning, OverheadRig,
            ZooCamera,
        },
        math::{
            advance_pan_accumulators, apply_camera_relative_pan_to_overhead_focus,
            apply_overhead_zoom_delta, approach_scalar, clamp_rig, consume_wheel_zoom,
            overhead_eye_and_target, overhead_pose,
        },
    },
    camera_ground_fit_surface_sampling::sample_highest_active_collision_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood,
    camera_pose_comparison::camera_pose_materially_changed,
    overhead_camera_zoom_policy::overhead_camera_zoom_speed_multiplier,
};

pub(in crate::plugins::camera) fn advance_overhead_camera(
    time: Res<Time<Real>>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    spatial_query: SpatialQuery,
    mut cameras: Query<
        (
            &mut Transform,
            &mut Projection,
            &mut OverheadRig,
            &CameraTuning,
            &CameraBounds,
            &mut CameraIntent,
            &CameraMode,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let dt = time.delta_secs();
    for (mut transform, mut projection, mut rig, tuning, bounds, mut intent, mode) in &mut cameras {
        if *mode != CameraMode::Overhead {
            continue;
        }
        if !dt.is_finite() || dt <= 0.0 || !tuning.is_valid() || !bounds.is_valid() {
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
        let wheel_zoom = consume_wheel_zoom(&mut intent.wheel_zoom_seconds, dt);

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
            let requested_scaling = ScalingMode::FixedVertical {
                viewport_height: rig.distance * tuning.vertical_view_per_zoom,
            };
            if !matches!(&current.scaling_mode, ScalingMode::FixedVertical { viewport_height }
                if *viewport_height == rig.distance * tuning.vertical_view_per_zoom)
            {
                if let Projection::Orthographic(projection) = &mut *projection {
                    projection.scaling_mode = requested_scaling;
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
        let required_camera_offset =
            sample_highest_active_collision_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
                unfit_eye.xz(),
                &spatial_query,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            )
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

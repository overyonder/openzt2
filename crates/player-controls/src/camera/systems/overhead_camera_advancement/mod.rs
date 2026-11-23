use avian3d::prelude::{
    Collider, ColliderAabb, ColliderDisabled, Position, Rotation, SpatialQuery,
};
use bevy::{camera::ScalingMode, prelude::*};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;

use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::advance_pan_accumulators;
use super::super::math::apply_camera_relative_pan_to_overhead_focus;
use super::super::math::apply_overhead_zoom_delta;
use super::super::math::approach_scalar;
use super::super::math::clamp_rig;
use super::super::math::consume_wheel_zoom;
use super::super::math::{overhead_unlifted_eye_and_target, WheelZoomQueue};
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

#[derive(Component, Default, PartialEq)]
pub(in crate::camera) struct OverheadUnliftedPose(Option<(Vec3, Vec3)>);

#[derive(Component, Default, PartialEq)]
pub(in crate::camera) struct OverheadFittingSurface {
    terrain_height: Option<f32>,
    collider: Option<(f32, Entity)>,
}

pub(in crate::camera) fn advance_overhead_camera_controls(
    time: Res<Time<Real>>,
    mut cameras: Query<
        (
            &CameraMode,
            &mut OverheadRig,
            Ref<CameraTuning>,
            Ref<CameraBounds>,
            &mut CameraIntent,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let dt = time.delta_secs();
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    for (mode, mut rig, tuning, bounds, mut intent) in &mut cameras {
        if *mode != CameraMode::Overhead {
            continue;
        }
        let moving = intent.pan_directions != [0.0; 4]
            || rig.pan_accumulators != [0.0; 4]
            || intent.turn != 0.0
            || intent.pitch != 0.0
            || intent.zoom != 0.0
            || intent.wheel_zoom != WheelZoomQueue::Idle;
        if !moving && !tuning.is_changed() && !bounds.is_changed() {
            continue;
        }
        let mut next = rig.clone();
        let speed = overhead_camera_zoom_speed_multiplier(
            next.distance,
            next.minimum_zoom_offset_m,
            &tuning,
        );
        let pan_axis = advance_pan_accumulators(
            &mut next.pan_accumulators,
            intent.pan_directions,
            tuning.pan_start_rate,
            tuning.pan_stop_rate,
            dt,
        );
        let mut wheel_queue = intent.wheel_zoom;
        let wheel_zoom = consume_wheel_zoom(&mut wheel_queue, dt);
        let turn = intent.turn;
        let pitch = intent.pitch;
        let zoom = intent.zoom;
        intent
            .map_unchanged(|intent| &mut intent.wheel_zoom)
            .set_if_neq(wheel_queue);
        apply_camera_relative_pan_to_overhead_focus(
            &mut next,
            pan_axis * tuning.pan_speed_mps * speed * dt,
        );
        next.yaw += turn * tuning.turn_speed_rps * dt;
        next.pitch += pitch * tuning.turn_speed_rps * dt;
        apply_overhead_zoom_delta(
            &mut next,
            &tuning,
            bounds.camera_clearance_m,
            -(zoom + wheel_zoom) * tuning.zoom_speed_mps * speed * dt,
        );
        clamp_rig(&mut next, &tuning, &bounds);
        rig.set_if_neq(next);
    }
}

pub(in crate::camera) fn project_unlifted_overhead_camera_pose(
    mut cameras: Query<
        (&CameraMode, &OverheadRig, &mut OverheadUnliftedPose),
        (
            With<ZooCamera>,
            Or<(
                Changed<CameraMode>,
                Changed<OverheadRig>,
                Added<OverheadUnliftedPose>,
            )>,
        ),
    >,
) {
    for (mode, rig, mut pose) in &mut cameras {
        let next = (*mode == CameraMode::Overhead).then(|| overhead_unlifted_eye_and_target(rig));
        pose.set_if_neq(OverheadUnliftedPose(next));
    }
}

#[allow(clippy::type_complexity)]
pub(in crate::camera) fn sample_overhead_camera_fitting_surface(
    terrain_index: Single<Ref<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    spatial_query: SpatialQuery,
    changed_terrain: Query<(), Or<(Changed<TerrainChunk>, Changed<EditedTerrainSamples>)>>,
    (mut removed_edits, mut removed_chunks): (
        RemovedComponents<EditedTerrainSamples>,
        RemovedComponents<TerrainChunk>,
    ),
    active_colliders: Query<(), (With<Collider>, Without<ColliderDisabled>)>,
    changed_colliders: Query<
        (),
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
    mut cameras: Query<(Ref<OverheadUnliftedPose>, &mut OverheadFittingSurface), With<ZooCamera>>,
) {
    let terrain_index = terrain_index.into_inner();
    let removed_terrain: bevy::ecs::entity::EntityHashSet =
        removed_edits.read().chain(removed_chunks.read()).collect();
    let reenabled: bevy::ecs::entity::EntityHashSet = enabled_colliders.read().collect();
    let terrain_reloaded = terrain_index.is_changed() || terrain_assets.is_changed();
    let test_local_colliders = !changed_colliders.is_empty() || !reenabled.is_empty();
    for (pose, mut sample) in &mut cameras {
        let Some((eye, _)) = pose.0 else { continue };
        let probe = eye.xz();
        let terrain_local_change = (!changed_terrain.is_empty() || !removed_terrain.is_empty())
            && [-1.0, 0.0, 1.0].into_iter().any(|z| {
                [-1.0, 0.0, 1.0].into_iter().any(|x| {
                    terrain_chunk_at(&terrain_index, probe + Vec2::new(x, z)).is_some_and(|chunk| {
                        changed_terrain.contains(chunk) || removed_terrain.contains(&chunk)
                    })
                })
            });
        let terrain_height = if pose.is_changed()
            || terrain_reloaded
            || terrain_local_change
            || sample.is_added()
        {
            sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(probe, &terrain_index, &terrain_assets, &terrain_chunks)
        } else {
            sample.terrain_height
        };
        let previous_changed = sample.collider.is_some_and(|(_, collider)| {
            !active_colliders.contains(collider) || changed_colliders.contains(collider)
        });
        let collider = if pose.is_changed()
            || sample.is_added()
            || previous_changed
            || test_local_colliders && {
                let neighborhood = ColliderAabb::from_min_max(
                    Vec3::new(probe.x - 1.0, -10_000.0, probe.y - 1.0),
                    Vec3::new(probe.x + 1.0, 10_000.0, probe.y + 1.0),
                );
                let mut changed_nearby = false;
                spatial_query.aabb_intersections_with_aabb_callback(neighborhood, |collider| {
                    if changed_colliders.contains(collider) || reenabled.contains(&collider) {
                        changed_nearby = true;
                    }
                    !changed_nearby
                });
                changed_nearby
            } {
            sample_highest_camera_fitting_collider(probe, &spatial_query)
        } else {
            sample.collider
        };
        sample.set_if_neq(OverheadFittingSurface {
            terrain_height,
            collider,
        });
    }
}

pub(in crate::camera) fn fit_and_present_overhead_camera_pose(
    time: Res<Time<Real>>,
    mut completed_transitions: RemovedComponents<CameraTransition>,
    mut cameras: Query<
        (
            &mut Transform,
            &mut Projection,
            &mut OverheadRig,
            Ref<CameraTuning>,
            Ref<CameraBounds>,
            Ref<OverheadUnliftedPose>,
            Ref<OverheadFittingSurface>,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let completed: bevy::ecs::entity::EntityHashSet = completed_transitions.read().collect();
    let dt = time.delta_secs();
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    for (mut transform, mut projection, mut rig, tuning, bounds, pose, surface) in &mut cameras {
        let Some((eye, target)) = pose.0 else {
            continue;
        };
        let required_offset = surface
            .terrain_height
            .into_iter()
            .chain(surface.collider.map(|(height, _)| height))
            .reduce(f32::max)
            .map(|height| {
                (height + bounds.camera_clearance_m - tuning.collision_radius_m - eye.y).max(0.0)
            });
        let needs_fit =
            required_offset.is_some_and(|offset| offset != rig.camera_ground_fit_offset_m);
        if !needs_fit
            && !pose.is_changed()
            && !surface.is_changed()
            && !tuning.is_changed()
            && !bounds.is_changed()
            && !rig.is_changed()
            && completed.is_empty()
        {
            continue;
        }
        let fit = required_offset.map_or(rig.camera_ground_fit_offset_m, |offset| {
            advance_authored_overhead_camera_node_soft_fit(
                rig.camera_ground_fit_offset_m,
                offset,
                &tuning,
                dt,
            )
        });
        let target_fit = if tuning.no_tilt_on_fit { fit } else { 0.0 };
        rig.reborrow()
            .map_unchanged(|rig| &mut rig.camera_ground_fit_offset_m)
            .set_if_neq(fit);
        rig.reborrow()
            .map_unchanged(|rig| &mut rig.target_ground_fit_offset_m)
            .set_if_neq(target_fit);
        let viewport_height = rig.distance * tuning.vertical_view_per_zoom;
        if matches!(&*projection, Projection::Orthographic(current) if !matches!(current.scaling_mode, ScalingMode::FixedVertical { viewport_height: height } if height == viewport_height))
        {
            if let Projection::Orthographic(current) = &mut *projection {
                current.scaling_mode = ScalingMode::FixedVertical { viewport_height };
            }
        }
        let next = Transform::from_translation(eye + Vec3::Y * fit)
            .looking_at(target + Vec3::Y * target_fit, Vec3::Y);
        if camera_pose_materially_changed(&transform, &next) {
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

use std::f32::consts::PI;

use bevy::prelude::*;

use super::{
    camera_runtime_state_types::{CameraEasing, CameraTransition, CameraTuning, OverheadRig},
    math::{
        advance_pan_accumulators, advance_transition, apply_camera_relative_pan_to_overhead_focus,
        apply_overhead_zoom_delta, consume_wheel_zoom, overhead_pose, queue_wheel_zoom,
        WheelZoomQueue,
    },
};

fn camera_tuning_for_calculation_tests() -> CameraTuning {
    CameraTuning {
        distance: 8.0..=80.0,
        maximum_distance_by_detail: [40.0, 60.0, 80.0],
        vertical_view_per_zoom: 0.96,
        pitch: 0.2..=1.2,
        yaw: -PI..=PI,
        pan_speed_mps: 20.0,
        turn_speed_rps: 2.0,
        zoom_speed_mps: 30.0,
        pan_start_rate: 0.4,
        pan_stop_rate: -2.5,
        soft_fit_distance_m: 0.5,
        soft_fit_speed_mps: 20.0,
        no_tilt_on_fit: true,
        collision_radius_m: 0.2,
        parenting_offset_m: 9.0,
        edge_scroll: true,
        zoom_speed_samples: [Vec2::ZERO; 8],
        zoom_speed_sample_count: 0,
        sub_zero_zoom_multiplier: 1.0,
    }
}

fn overhead_camera_rig_for_calculation_tests() -> OverheadRig {
    OverheadRig {
        focus: Vec2::ZERO,
        height_m: 0.0,
        look_at_height_m: 0.2,
        yaw: 0.0,
        pitch: 0.7,
        distance: 32.0,
        look_at_distance_m: 12.0,
        minimum_zoom_offset_m: 0.0,
        camera_ground_fit_offset_m: 0.0,
        target_ground_fit_offset_m: 0.0,
        pan_accumulators: [0.0; 4],
    }
}

#[test]
fn overhead_pan_directions_follow_the_rendered_camera_at_every_quarter_turn() {
    for yaw in [-PI, -PI * 0.5, 0.0, PI * 0.5, PI, 1.1] {
        for input in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
            let mut rig = overhead_camera_rig_for_calculation_tests();
            rig.yaw = yaw;
            let before = overhead_pose(&rig);
            let right = before.right().xz().normalize();
            let forward = before.forward().xz().normalize();
            let expected_displacement = right * input.x + forward * input.y;

            apply_camera_relative_pan_to_overhead_focus(&mut rig, input);

            let actual_displacement = (overhead_pose(&rig).translation - before.translation).xz();
            assert!(actual_displacement.abs_diff_eq(expected_displacement, 0.000_01));
        }
    }
}

#[test]
fn positive_source_yaw_turns_the_converted_overhead_camera_branch_toward_negative_world_x() {
    let mut overhead_camera_rig = overhead_camera_rig_for_calculation_tests();
    overhead_camera_rig.yaw = std::f32::consts::FRAC_PI_2;
    let pose = overhead_pose(&overhead_camera_rig);
    assert!(pose.translation.x < 0.0);
    assert!(pose.translation.z.abs() < 0.000_01);
}

#[test]
fn pan_ramp_stops_immediately_and_retains_decay_state() {
    let mut pan_accumulators = [0.0; 4];
    let first_pan_axis =
        advance_pan_accumulators(&mut pan_accumulators, [1.0, 0.0, 0.0, 0.0], 0.4, -2.5, 0.25);
    assert!((first_pan_axis.y - (std::f32::consts::FRAC_PI_2 * 0.1).sin()).abs() < 1e-6);
    for _ in 0..9 {
        advance_pan_accumulators(&mut pan_accumulators, [1.0, 0.0, 0.0, 0.0], 0.4, -2.5, 0.25);
    }
    assert_eq!(pan_accumulators[0], 1.0);

    let first_release_axis =
        advance_pan_accumulators(&mut pan_accumulators, [0.0; 4], 0.4, -2.5, 0.2);
    assert_eq!(first_release_axis, Vec2::ZERO);
    assert_eq!(pan_accumulators[0], 0.5);
    let final_release_axis =
        advance_pan_accumulators(&mut pan_accumulators, [0.0; 4], 0.4, -2.5, 0.2);
    assert_eq!(final_release_axis, Vec2::ZERO);
    assert_eq!(pan_accumulators[0], 0.0);
}

#[test]
fn pan_ramp_reaches_the_same_full_scale_at_60_and_144_hz() {
    fn hold_pan_for_frames(frames: u32, delta_seconds: f32) -> ([f32; 4], Vec2) {
        let mut pan_accumulators = [0.0; 4];
        let mut pan_axis = Vec2::ZERO;
        for _ in 0..frames {
            pan_axis = advance_pan_accumulators(
                &mut pan_accumulators,
                [1.0, 0.0, 1.0, 0.0],
                0.4,
                -2.5,
                delta_seconds,
            );
        }
        (pan_accumulators, pan_axis)
    }

    let (pan_accumulators_at_60_hz, pan_axis_at_60_hz) = hold_pan_for_frames(150, 1.0 / 60.0);
    let (pan_accumulators_at_144_hz, pan_axis_at_144_hz) = hold_pan_for_frames(360, 1.0 / 144.0);
    assert!((pan_accumulators_at_60_hz[0] - 1.0).abs() < 1e-5);
    assert!((pan_accumulators_at_144_hz[0] - 1.0).abs() < 1e-5);
    assert!(pan_axis_at_60_hz.abs_diff_eq(Vec2::ONE, 1e-5));
    assert!(pan_axis_at_144_hz.abs_diff_eq(Vec2::ONE, 1e-5));
}

#[test]
fn one_wheel_notch_queues_eight_hundredths_at_double_zoom_rate() {
    let integrated_zoom = |hertz: f32| {
        let mut queue = WheelZoomQueue::Idle;
        queue_wheel_zoom(&mut queue, 1.0);
        let mut integrated = 0.0;
        while queue != WheelZoomQueue::Idle {
            integrated += consume_wheel_zoom(&mut queue, 1.0 / hertz) / hertz;
        }
        integrated
    };
    assert!((integrated_zoom(60.0) - 0.16).abs() <= 2.0 / 60.0);
    assert!((integrated_zoom(144.0) - 0.16).abs() <= 2.0 / 144.0);
}

#[test]
fn minimum_zoom_retains_and_then_consumes_the_close_in_residual() {
    let camera_tuning = camera_tuning_for_calculation_tests();
    let mut overhead_camera_rig = overhead_camera_rig_for_calculation_tests();
    overhead_camera_rig.distance = *camera_tuning.distance.start();

    apply_overhead_zoom_delta(&mut overhead_camera_rig, &camera_tuning, 0.0, -2.0);
    assert_eq!(overhead_camera_rig.distance, 8.0);
    assert_eq!(overhead_camera_rig.minimum_zoom_offset_m, -2.0);

    apply_overhead_zoom_delta(&mut overhead_camera_rig, &camera_tuning, 0.0, 1.25);
    assert_eq!(overhead_camera_rig.distance, 8.0);
    assert_eq!(overhead_camera_rig.minimum_zoom_offset_m, -0.75);

    apply_overhead_zoom_delta(&mut overhead_camera_rig, &camera_tuning, 0.0, 2.0);
    assert_eq!(overhead_camera_rig.distance, 9.25);
    assert_eq!(overhead_camera_rig.minimum_zoom_offset_m, 0.0);
}

#[test]
fn overhead_pose_flattens_the_authored_radial_node_chain() {
    let overhead_camera_rig = overhead_camera_rig_for_calculation_tests();
    let overhead_camera_pose = overhead_pose(&overhead_camera_rig);
    let authored_radial_distance =
        overhead_camera_rig.distance + overhead_camera_rig.look_at_distance_m;
    let expected_eye = Vec3::new(
        0.0,
        authored_radial_distance * overhead_camera_rig.pitch.sin(),
        authored_radial_distance * overhead_camera_rig.pitch.cos(),
    );
    let expected_focus = Vec3::new(0.0, 0.2, 0.0);

    assert!(overhead_camera_pose
        .translation
        .abs_diff_eq(expected_eye, 1e-5));
    assert!(
        overhead_camera_pose
            .forward()
            .dot((expected_focus - expected_eye).normalize())
            > 1.0 - 1e-5
    );
}

#[test]
fn terrain_fit_lift_remains_independent_of_close_in_zoom_demand() {
    let mut overhead_camera_rig = overhead_camera_rig_for_calculation_tests();
    overhead_camera_rig.minimum_zoom_offset_m = -3.0;
    overhead_camera_rig.camera_ground_fit_offset_m = 4.0;
    overhead_camera_rig.target_ground_fit_offset_m = 4.0;

    let overhead_camera_pose = overhead_pose(&overhead_camera_rig);
    let authored_radial_distance =
        overhead_camera_rig.distance + overhead_camera_rig.look_at_distance_m;
    let expected_eye_height = 1.0 + authored_radial_distance * overhead_camera_rig.pitch.sin();

    assert!((overhead_camera_pose.translation.y - expected_eye_height).abs() < 1e-5);
}

#[test]
fn transition_writes_the_exact_endpoint() {
    let transition_start = Transform::from_xyz(1.0, 2.0, 3.0);
    let transition_end =
        Transform::from_xyz(-7.0, 11.0, 5.0).with_rotation(Quat::from_rotation_y(2.4));
    let mut camera_transition = CameraTransition {
        from: transition_start,
        to: transition_end,
        elapsed: 0.0,
        duration: 0.75,
        easing: CameraEasing::SmoothStep,
    };
    let mut actual_camera_transform = transition_start;
    assert!(advance_transition(
        &mut camera_transition,
        &mut actual_camera_transform,
        2.0
    ));
    assert_eq!(actual_camera_transform, transition_end);
}

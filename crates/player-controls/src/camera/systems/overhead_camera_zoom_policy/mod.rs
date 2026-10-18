use bevy::prelude::*;

use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail;

use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use crate::camera::camera_execution_types::CameraTuning;

/// Applies the original dedicated overhead-camera distance option to the
/// authored low/medium/high limits without changing the current focus.
pub(crate) fn apply_overhead_camera_zoom_graphics_setting(
    graphics: Res<GraphicsSettings>,
    mut cameras: Query<(&mut CameraTuning, &mut OverheadRig, &CameraMode), With<ZooCamera>>,
) {
    if !graphics.is_changed() {
        return;
    }
    for (mut tuning, mut rig, mode) in &mut cameras {
        if *mode != CameraMode::Overhead {
            continue;
        }
        let maximum = select_three_level_graphics_detail_value(
            tuning.maximum_distance_by_detail,
            graphics.max_overhead_zoom,
        );
        let minimum = *tuning.distance.start();
        tuning.distance = minimum..=maximum;
        rig.distance = rig.distance.min(maximum);
    }
}

/// Picks the authored low, medium, or high tier of a setting-dependent value.
pub(in crate::camera) const fn select_three_level_graphics_detail_value(
    maximums: [f32; 3],
    detail: ThreeLevelGraphicsDetail,
) -> f32 {
    maximums[match detail {
        ThreeLevelGraphicsDetail::Low => 0,
        ThreeLevelGraphicsDetail::Medium => 1,
        ThreeLevelGraphicsDetail::High => 2,
    }]
}

pub(super) fn overhead_camera_zoom_speed_multiplier(
    distance: f32,
    minimum_zoom_offset_m: f32,
    tuning: &CameraTuning,
) -> f32 {
    if distance <= *tuning.distance.start() && minimum_zoom_offset_m < 0.0 {
        return tuning.sub_zero_zoom_multiplier;
    }
    let span = *tuning.distance.end() - *tuning.distance.start();
    let normalized = if span > 0.0 {
        ((distance - *tuning.distance.start()) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let samples = &tuning.zoom_speed_samples[..usize::from(tuning.zoom_speed_sample_count)];
    let Some(first) = samples.first() else {
        return 1.0;
    };
    if normalized <= first.x {
        return first.y;
    }
    samples
        .windows(2)
        .find(|pair| normalized <= pair[1].x)
        .map(|pair| {
            let t = (normalized - pair[0].x) / (pair[1].x - pair[0].x);
            pair[0].y + (pair[1].y - pair[0].y) * t
        })
        .unwrap_or_else(|| samples.last().map_or(1.0, |sample| sample.y))
}

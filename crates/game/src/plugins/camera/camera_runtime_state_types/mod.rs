use std::ops::RangeInclusive;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ZooCamera;

/// Presentation-only mouse-look state on the active zoo camera.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CameraMouseLook;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CameraDefinition(pub(crate) AssetId);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CameraMode {
    #[default]
    Overhead,
    Follow(Entity),
    FirstPerson(Entity),
    Free,
}

impl CameraMode {
    pub(crate) fn subject(self) -> Option<Entity> {
        match self {
            Self::Follow(entity) | Self::FirstPerson(entity) => Some(entity),
            Self::Overhead | Self::Free => None,
        }
    }

    pub(crate) fn is_immersive(self) -> bool {
        !matches!(self, Self::Overhead)
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct OverheadRig {
    pub(crate) focus: Vec2,
    /// Saved world height of the overhead hierarchy root. Ground fitting
    /// changes the residual node offset rather than replacing this value.
    pub(crate) height_m: f32,
    /// Authored vertical separation between the saved hierarchy root and the
    /// sibling look-at target.
    pub(crate) look_at_height_m: f32,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) distance: f32,
    /// Authored horizontal separation between the world pivot and the pitched
    /// camera/target node.
    pub(crate) look_at_distance_m: f32,
    /// Signed zoom demand retained while radial distance is clamped at the
    /// authored minimum.
    pub(crate) minimum_zoom_offset_m: f32,
    /// Active fitting-surface lift applied to the native camera node
    /// independently of close-in zoom demand.
    pub(crate) camera_ground_fit_offset_m: f32,
    /// Active fitting-surface lift applied to the native look-at target node.
    /// The authored `noTiltOnFit` policy may keep this equal to the camera-node
    /// lift without collapsing the two node fits into one sample.
    pub(crate) target_ground_fit_offset_m: f32,
    /// Original UDV directional ramps: forward, backward, right, left.
    pub(crate) pan_accumulators: [f32; 4],
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct CameraBounds {
    pub(crate) min: Vec2,
    pub(crate) max: Vec2,
    pub(crate) camera_clearance_m: f32,
}

impl CameraBounds {
    pub(crate) fn is_valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.cmple(self.max).all()
            && self.camera_clearance_m.is_finite()
            && self.camera_clearance_m >= 0.0
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct CameraTuning {
    pub(crate) distance: RangeInclusive<f32>,
    /// Authored maximum distance for the low, medium, and high
    /// `maxOverheadZoom` settings.
    pub(crate) maximum_distance_by_detail: [f32; 3],
    /// Orthographic vertical world extent produced by one rig-distance unit.
    /// Zero denotes a perspective camera.
    pub(crate) vertical_view_per_zoom: f32,
    pub(crate) pitch: RangeInclusive<f32>,
    pub(crate) yaw: RangeInclusive<f32>,
    pub(crate) pan_speed_mps: f32,
    pub(crate) turn_speed_rps: f32,
    pub(crate) zoom_speed_mps: f32,
    pub(crate) pan_start_rate: f32,
    pub(crate) pan_stop_rate: f32,
    pub(crate) soft_fit_distance_m: f32,
    pub(crate) soft_fit_speed_mps: f32,
    pub(crate) no_tilt_on_fit: bool,
    pub(crate) collision_radius_m: f32,
    /// Authored subject-relative height used when the overhead camera is
    /// parented to a followed world entity.
    pub(crate) parenting_offset_m: f32,
    pub(crate) edge_scroll: bool,
    pub(crate) zoom_speed_samples: [Vec2; 8],
    pub(crate) zoom_speed_sample_count: u8,
    pub(crate) sub_zero_zoom_multiplier: f32,
}

impl CameraTuning {
    pub(crate) fn is_valid(&self) -> bool {
        valid_range(&self.distance)
            && self.vertical_view_per_zoom.is_finite()
            && self.vertical_view_per_zoom >= 0.0
            && valid_range(&self.pitch)
            && valid_range(&self.yaw)
            && self.pan_speed_mps.is_finite()
            && self.pan_speed_mps >= 0.0
            && self.turn_speed_rps.is_finite()
            && self.turn_speed_rps >= 0.0
            && self.zoom_speed_mps.is_finite()
            && self.zoom_speed_mps >= 0.0
            && positive_finite(self.pan_start_rate)
            && self.pan_stop_rate.is_finite()
            && self.pan_stop_rate < 0.0
            && self.soft_fit_distance_m.is_finite()
            && self.soft_fit_distance_m >= 0.0
            && self.soft_fit_speed_mps.is_finite()
            && self.soft_fit_speed_mps >= 0.0
            && self.collision_radius_m.is_finite()
            && self.collision_radius_m >= 0.0
            && self.parenting_offset_m.is_finite()
            && self.parenting_offset_m >= 0.0
            && self.zoom_speed_sample_count <= 8
            && self.sub_zero_zoom_multiplier.is_finite()
            && self.sub_zero_zoom_multiplier > 0.0
            && self.zoom_speed_samples[..usize::from(self.zoom_speed_sample_count)]
                .iter()
                .all(|sample| sample.is_finite())
            && self
                .maximum_distance_by_detail
                .iter()
                .all(|maximum| maximum.is_finite() && *maximum >= *self.distance.start())
            && self
                .maximum_distance_by_detail
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CameraSpawnPending(pub(crate) CameraSpawnPendingReason);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CameraSpawnPendingReason {
    MissingMap,
    MissingStartingZoo,
    MissingCameraDefinition,
    InvalidCameraDefinition,
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct CameraTransition {
    pub(crate) from: Transform,
    pub(crate) to: Transform,
    pub(crate) elapsed: f32,
    pub(crate) duration: f32,
    pub(crate) easing: CameraEasing,
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct CameraReturnState {
    pub(crate) mode: CameraMode,
    pub(crate) definition: AssetId,
    pub(crate) transform: Transform,
    pub(crate) overhead: OverheadRig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CameraEasing {
    #[default]
    SmoothStep,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CameraIntent {
    pub(crate) pan_directions: [f32; 4],
    pub(crate) turn: f32,
    pub(crate) pitch: f32,
    pub(crate) zoom: f32,
    /// Zoom queued by mouse-wheel notches.
    pub(crate) wheel_zoom: super::math::WheelZoomQueue,
    /// Latched axes owned by authored UI press/release actions. The input system's
    /// frame-local input is combined with these without overwriting it.
    pub(crate) ui_pan: Vec2,
    pub(crate) ui_turn: f32,
    pub(crate) ui_zoom: f32,
    /// Independent held states of the authored photo commands.
    pub(crate) photo_zoom_in_command: bool,
    pub(crate) photo_zoom_out_command: bool,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RemoveReturnStateOnComplete;

fn positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn valid_range(range: &RangeInclusive<f32>) -> bool {
    range.start().is_finite() && range.end().is_finite() && range.start() <= range.end()
}

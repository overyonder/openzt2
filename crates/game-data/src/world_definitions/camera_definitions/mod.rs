//! Authored camera projection and movement tuning definitions.

use crate::AssetId;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum CameraProjectionKind {
    Perspective,
    Orthographic,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct CameraTuningDefinition {
    pub id: AssetId,
    pub projection: CameraProjectionKind,
    pub fov_y_radians: f32,
    /// Authored fixed width/height ratio of a perspective frustum. The source
    /// renderer stretches this projection over the selected display mode.
    /// Orthographic definitions leave it at zero.
    pub fixed_aspect_ratio: f32,
    /// Orthographic vertical world extent produced by one authored zoom unit.
    /// Perspective cameras leave this at zero.
    pub vertical_view_per_zoom: f32,
    pub near_m: f32,
    pub far_m: f32,
    pub offset_m: [f32; 3],
    pub pitch_radians: [f32; 2],
    pub yaw_radians: [f32; 2],
    pub minimum_zoom_m: f32,
    /// Authored maximum overhead distance for the low, medium, and high
    /// `maxOverheadZoom` settings. Definitions without tiered source values
    /// repeat their one maximum in all three slots.
    pub maximum_zoom_m: [f32; 3],
    pub collision_radius_cm: u16,
    pub initial_pitch_radians: f32,
    pub initial_zoom_m: f32,
    pub look_at_distance_m: f32,
    pub parenting_offset_m: f32,
    pub pan_speed_mps: f32,
    pub turn_speed_rps: f32,
    pub zoom_speed_mps: f32,
    /// Rate per second at which each active directional pan accumulator rises.
    pub pan_start_rate: f32,
    /// Signed rate per second at which each inactive directional pan accumulator falls.
    pub pan_stop_rate: f32,
    pub ground_buffer_m: f32,
    pub soft_fit_distance_m: f32,
    pub soft_fit_speed_mps: f32,
    pub no_tilt_on_fit: bool,
    pub edge_scroll: bool,
    pub zoom_speed_samples: [[f32; 2]; 8],
    pub zoom_speed_sample_count: u8,
    pub sub_zero_zoom_multiplier: f32,
    /// Maximum on-land acceleration of a walking first-person camera.
    /// Cameras without a first-person ground fit leave this at zero.
    #[serde(default)]
    pub ground_acceleration_mps2: f32,
}

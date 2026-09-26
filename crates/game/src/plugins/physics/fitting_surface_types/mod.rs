//! Authored fitting-surface policy, entity providers, and sampled results.

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TerrainFittingSurface {
    pub(crate) normal_tolerance: f32,
}

/// Presentation-only crossing history for an authored real-physics body.
/// Avian owns motion; this retains only the previous sampled body-bottom point
/// needed to emit the original one-shot water-surface impact transition.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct AuthoredWaterSurfaceCrossingProjectionState {
    pub(super) previous_body_bottom_world_position: Option<Vec3>,
    pub(super) previous_water_surface_height: Option<f32>,
}

use bevy::prelude::*;
use openzt2_game_data::{world_definitions::environment::EnvironmentLightTarget, AssetId};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EnvironmentLight {
    pub(crate) keyframe: u16,
    /// Which part of the scene this light illuminates.
    pub(crate) target: EnvironmentLightTarget,
    /// Primitive rank of the authored light kind. Zero is the consumer-shaped
    /// single-light curve; legacy rigs use one, two, and three for sun, side,
    /// and back respectively.
    pub(crate) kind_rank: u8,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentSky {
    pub(super) definition: AssetId,
}

/// Texture and material for an environmental visual.
#[derive(Component, Clone, Debug)]
pub(super) struct EnvironmentTexture {
    pub(super) image: Handle<Image>,
    pub(super) material: Option<Handle<StandardMaterial>>,
}

/// Marks an environmental model visual.
#[derive(Component, Clone, Debug)]
pub(super) struct EnvironmentModelVisual;

/// Root of an authored sky presentation which remains centred on the active
/// camera while retaining its own world orientation.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(super) struct EnvironmentCameraRelativeVisual {
    pub(super) scale_to_camera_far_plane: bool,
    pub(super) projected_scale: f32,
    pub(super) source_radius_m: Option<f32>,
}

impl EnvironmentCameraRelativeVisual {
    pub(super) fn sky_layer(projected_scale: f32) -> Self {
        Self {
            scale_to_camera_far_plane: true,
            projected_scale,
            source_radius_m: None,
        }
    }

    pub(super) fn sun_position_rig() -> Self {
        Self {
            scale_to_camera_far_plane: false,
            projected_scale: 1.0,
            source_radius_m: None,
        }
    }
}

/// Index of a timed layer in the environment's visual table.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentVisualSampleIndex(pub(super) u32);

/// Links a timed visual to the Bevy environment entity that owns it. Visuals
/// may be parented to an authored model joint, so hierarchy depth cannot stand
/// in for this relationship.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentVisualOwner(pub(super) Entity);

/// One authored sun body awaiting its named joint in a shared position rig.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentSunPosition {
    pub(super) rig: Entity,
    pub(super) node: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EnvironmentFog;

/// One-frame marker ensuring freshly hydrated renderer components receive the
/// current sample even when the clock has not advanced.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct EnvironmentPresentationPending;

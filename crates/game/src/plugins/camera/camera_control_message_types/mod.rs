use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::camera_runtime_state_types::CameraMode;

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct SetCameraMode {
    pub(crate) mode: CameraMode,
    pub(crate) definition: AssetId,
    pub(crate) transition_seconds: Option<f32>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct RestoreCameraMode {
    pub(crate) transition_seconds: Option<f32>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct FocusCamera {
    pub(crate) world: Vec3,
    pub(crate) transition_seconds: Option<f32>,
}

/// Positions the free camera in world space.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct PositionFreeCamera {
    pub(crate) world: Vec3,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct AimFreeCameraAt {
    pub(crate) world: Vec3,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraMoved {
    pub(crate) eye: Vec3,
    pub(crate) focus: Vec3,
}

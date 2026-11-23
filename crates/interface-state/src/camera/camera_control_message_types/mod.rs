use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::camera_runtime_state_types::CameraMode;

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct SetCameraMode {
    pub mode: CameraMode,
    /// A standalone target. A relative mode without one uses its admitted session's subject.
    pub subject: Option<Entity>,
    pub definition: AssetId,
    pub transition_seconds: Option<f32>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct RestoreCameraMode {
    pub transition_seconds: Option<f32>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct FocusCamera {
    pub world: Vec3,
    pub transition_seconds: Option<f32>,
}

/// Positions the free camera in world space.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PositionFreeCamera {
    pub world: Vec3,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct AimFreeCameraAt {
    pub world: Vec3,
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct CameraMoved {
    pub eye: Vec3,
    pub focus: Vec3,
}

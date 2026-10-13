use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZooCamera;

/// Presentation-only mouse-look state on the active zoo camera.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CameraMouseLook;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraDefinition(pub AssetId);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMode {
    #[default]
    Overhead,
    Follow(Entity),
    FirstPerson(Entity),
    Free,
}

impl CameraMode {
    pub fn subject(self) -> Option<Entity> {
        match self {
            Self::Follow(entity) | Self::FirstPerson(entity) => Some(entity),
            Self::Overhead | Self::Free => None,
        }
    }

    pub fn is_immersive(self) -> bool {
        !matches!(self, Self::Overhead)
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct OverheadRig {
    pub focus: Vec2,
    /// Saved world height of the overhead hierarchy root. Ground fitting
    /// changes the residual node offset rather than replacing this value.
    pub height_m: f32,
    /// Authored vertical separation between the saved hierarchy root and the
    /// sibling look-at target.
    pub look_at_height_m: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Authored horizontal separation between the world pivot and the pitched
    /// camera/target node.
    pub look_at_distance_m: f32,
    /// Signed zoom demand retained while radial distance is clamped at the
    /// authored minimum.
    pub minimum_zoom_offset_m: f32,
    /// Active fitting-surface lift applied to the native camera node
    /// independently of close-in zoom demand.
    pub camera_ground_fit_offset_m: f32,
    /// Active fitting-surface lift applied to the native look-at target node.
    /// The authored `noTiltOnFit` policy may keep this equal to the camera-node
    /// lift without collapsing the two node fits into one sample.
    pub target_ground_fit_offset_m: f32,
    /// Original UDV directional ramps: forward, backward, right, left.
    pub pan_accumulators: [f32; 4],
}

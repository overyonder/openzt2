//! Camera following, panning, focusing, mouse-look, zoom, rotation, and view actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiCameraActionRecord {
    pub trigger: UiTrigger,
    pub action: UiCameraAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiCameraAction {
    RestorePreviouslySavedCameraMode,
    StartFollowingSelectedEntity,
    SetCameraPanDirection {
        direction: UiCameraPanDirection,
    },
    FocusCameraOnSelectedEntity,
    /// Focus the overhead camera at the pointer's authored overview-map
    /// position. This action consumes the activated map node's UI geometry and
    /// never depends on entity selection.
    FocusCameraOnActivatedOverviewMapPoint,
    SetCameraMouseLookEnabled {
        enabled: bool,
    },
    ToggleCameraMouseLookEnabled,
    EnterFirstPersonCameraForSelectedEntity,
    SetPhotoZoomCommandState {
        direction: UiCameraSignedAxisDirection,
        pressed: bool,
    },
    SetCameraZoomDirection {
        direction: UiCameraSignedAxisDirection,
    },
    SetCameraRotationDirection {
        direction: UiCameraSignedAxisDirection,
    },
    StopCameraZoom,
    StopCameraRotation,
    StopCameraPan,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiCameraPanDirection {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiCameraSignedAxisDirection {
    Negative,
    Positive,
}

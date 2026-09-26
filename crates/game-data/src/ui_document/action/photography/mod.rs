//! Camera capture, photo album, picture selection, movement, deletion, and safari actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiPhotoActionRecord {
    pub trigger: UiTrigger,
    pub action: UiPhotoAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiPhotoAction {
    CapturePhotoFromActivePhotoModeCamera,
    ExitActivePhotoMode,
    RefreshPresentedActivePhotoAlbumPhotoCount,
    RefreshActivePhotoAlbumPage,
    EnlargeSelectedPhotoInActiveAlbum,
    StopEnlargingPhotoInActiveAlbum,
    ShowPreviousActivePhotoAlbumPage,
    ShowNextActivePhotoAlbumPage,
    CreateNewPhotoAlbumFromActiveAlbumProfile,
    MarkPhotoAlbumHelpAsShown,
    DeleteActivePhotoAlbumAndContainedPhotos,
    StartMovingSelectedPhotoToActiveAlbum,
    CancelMovingSelectedPhoto,
    ExportActivePhotoAlbumAsHtml,
    SelectPhotoInActiveAlbumSlot { album_slot_index: i32 },
    DeselectPhotoInActiveAlbumSlot { album_slot_index: i32 },
    EnlargePhotoInActiveAlbumSlot { album_slot_index: i32 },
    SelectPhotoInCameraRollSlot { camera_roll_slot_index: i32 },
    DeselectPhotoInCameraRollSlot { camera_roll_slot_index: i32 },
    SelectPhotoAlbumByListIndex { album_list_index: i32 },
    AddOnePageToActivePhotoAlbumCapacity,
    SetHoveredPhotoMoveTargetSlot { album_slot_index: i32 },
    ClearHoveredPhotoMoveTargetSlot { album_slot_index: i32 },
    MoveSelectedPhotoToActiveAlbumSlot { album_slot_index: i32 },
    DeleteAllPhotosFromCameraRoll,
    DeleteSelectedPhotoFromAnyAlbum,
    DeleteSelectedPhotoFromActiveAlbum,
    DeleteSelectedPhotoFromCameraRoll,
}

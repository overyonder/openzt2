use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    event_i32, required_photo_slot,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use openzt2_game_data::ui_document::action::photography::{UiPhotoAction, UiPhotoActionRecord};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_photo_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_PHOTOEVENT_NUM_PICTURES" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::RefreshPresentedActivePhotoAlbumPhotoCount,
        })),
        "ZT_PHOTOEVENT_ALBUM_PREVPAGE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::ShowPreviousActivePhotoAlbumPage,
        })),
        "ZT_PHOTOEVENT_ALBUM_NEXTPAGE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::ShowNextActivePhotoAlbumPage,
        })),
        "ZT_PHOTOEVENT_NEW_ALBUM" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::CreateNewPhotoAlbumFromActiveAlbumProfile,
        })),
        "ZT_SET_USER_ATTRIBUTE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::MarkPhotoAlbumHelpAsShown,
        })),
        "ZT_PHOTOEVENT_DELETE_ALBUM" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeleteActivePhotoAlbumAndContainedPhotos,
        })),
        "ZT_PHOTOEVENT_MOVE_SELECTED_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum,
        })),
        "ZT_PHOTOEVENT_ALBUM_SAVE_AS_HTML" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::ExportActivePhotoAlbumAsHtml,
        })),
        "ZT_PHOTOEVENT_ALBUM_SELECT_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::SelectPhotoInActiveAlbumSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_ALBUM_DESELECT_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeselectPhotoInActiveAlbumSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_ENLARGE_ALBUM_PIC" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::EnlargePhotoInActiveAlbumSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_CAMERA_SELECT_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::SelectPhotoInCameraRollSlot {
                camera_roll_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_CAMERA_DESELECT_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeselectPhotoInCameraRollSlot {
                camera_roll_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_SELECT_ALBUM" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::SelectPhotoAlbumByListIndex {
                album_list_index: event_i32(event, 0),
            },
        })),
        "ZT_PHOTOEVENT_ALBUM_INCREASE_SIZE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::AddOnePageToActivePhotoAlbumCapacity,
        })),
        "ZT_PHOTOEVENT_MOVE_ENTER" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::SetHoveredPhotoMoveTargetSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_MOVE_EXIT" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::ClearHoveredPhotoMoveTargetSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_MOVE_SELECTED_TARGET" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::MoveSelectedPhotoToActiveAlbumSlot {
                album_slot_index: required_photo_slot(event, input)?,
            },
        })),
        "ZT_PHOTOEVENT_CAMERA_DELETE_ALL_PICTURES" => {
            Ok(UiActionRecord::Photo(UiPhotoActionRecord {
                trigger,
                action: UiPhotoAction::DeleteAllPhotosFromCameraRoll,
            }))
        }
        "ZT_PHOTOEVENT_DELETE_SELECTED_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeleteSelectedPhotoFromAnyAlbum,
        })),
        "ZT_PHOTOEVENT_DELETE_ALBUM_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeleteSelectedPhotoFromActiveAlbum,
        })),
        "ZT_PHOTOEVENT_DELETE_CAMERA_PICTURE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::DeleteSelectedPhotoFromCameraRoll,
        })),
        "ZT_PHOTOEVENT_MOVE_SELECTED_PICTURE_CANCELED" => {
            Ok(UiActionRecord::Photo(UiPhotoActionRecord {
                trigger,
                action: UiPhotoAction::CancelMovingSelectedPhoto,
            }))
        }
        "ZT_PHOTOEVENT_TAKE_PHOTO" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::CapturePhotoFromActivePhotoModeCamera,
        })),
        "ZT_PHOTOEVENT_ALBUM" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::RefreshActivePhotoAlbumPage,
        })),
        "ZT_PHOTOEVENT_ENLARGE" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::EnlargeSelectedPhotoInActiveAlbum,
        })),
        "ZT_PHOTOEVENT_SHRINK" => Ok(UiActionRecord::Photo(UiPhotoActionRecord {
            trigger,
            action: UiPhotoAction::StopEnlargingPhotoInActiveAlbum,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

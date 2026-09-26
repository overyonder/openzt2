use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    camera_axis_direction, camera_pan_direction,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use openzt2_game_data::ui_document::action::camera::{
    UiCameraAction, UiCameraActionRecord, UiCameraSignedAxisDirection,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use std::io;

pub(super) fn lower_camera_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_COMMAND_STATE" | "ZT_CLEAR_COMMAND_STATE"
            if matches!(
                event.string.as_deref(),
                Some("photoZoomIn" | "photoZoomOut")
            ) =>
        {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::SetPhotoZoomCommandState {
                    // The source calls the positive scale command "photoZoomIn";
                    // its authored key is Minus. Preserve the authored pairing.
                    direction: if event.string.as_deref() == Some("photoZoomIn") {
                        UiCameraSignedAxisDirection::Positive
                    } else {
                        UiCameraSignedAxisDirection::Negative
                    },
                    pressed: event.message == "ZT_SET_COMMAND_STATE",
                },
            }))
        }
        "ZT_CAMERA_PAN" if event.string.as_deref().or(event.value.as_deref()) == Some("stop") => {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::StopCameraPan,
            }))
        }
        "ZT_CAMERA_POSITION" if role == UiDocumentRole::Overview => {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::FocusCameraOnActivatedOverviewMapPoint,
            }))
        }
        "ZT_MOUSELOOK" if event.string.as_deref().or(event.value.as_deref()) == Some("toggle") => {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::ToggleCameraMouseLookEnabled,
            }))
        }
        "ZT_CAMERA_ZOOM" if event.string.as_deref().or(event.value.as_deref()) == Some("stop") => {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::StopCameraZoom,
            }))
        }
        "ZT_CAMERA_ROTATE"
            if event.string.as_deref().or(event.value.as_deref()) == Some("stop") =>
        {
            Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::StopCameraRotation,
            }))
        }
        "ZT_STOP_FOLLOWCAM" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::RestorePreviouslySavedCameraMode,
        })),
        "ZT_START_FOLLOWCAM" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::StartFollowingSelectedEntity,
        })),
        "ZT_CAMERA_PAN" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::SetCameraPanDirection {
                direction: camera_pan_direction(event, input)?,
            },
        })),
        "ZT_CAMERA_POSITION" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::FocusCameraOnSelectedEntity,
        })),
        "ZT_MOUSELOOK" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::SetCameraMouseLookEnabled {
                enabled: match event.string.as_deref().or(event.value.as_deref()) {
                    Some(value) => parse_bool(value)
                        .ok_or_else(|| invalid_at(input, "ZT_MOUSELOOK requires toggle/on/off"))?,
                    None => true,
                },
            },
        })),
        "ZT_FP_TRACINGSTART" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::EnterFirstPersonCameraForSelectedEntity,
        })),
        "ZT_CAMERA_ZOOM" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::SetCameraZoomDirection {
                direction: camera_axis_direction(event, input)?,
            },
        })),
        "ZT_CAMERA_ROTATE" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
            trigger,
            action: UiCameraAction::SetCameraRotationDirection {
                direction: camera_axis_direction(event, input)?,
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

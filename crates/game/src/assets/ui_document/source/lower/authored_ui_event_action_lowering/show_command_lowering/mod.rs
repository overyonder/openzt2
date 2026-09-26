use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowActionRecord};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_show_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SHOWSCHEDULER_TOGGLEENABLE" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::ToggleSelectedEnabled,
        })),
        "ZT_SHOWSCHEDULER_ENTRYDOUBLECLICK" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::EditSelected,
        })),
        "ZT_SHOWMIXER_REQUEST_CANCELEDIT" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::CancelEdit,
        })),
        "ZT_SHOWMIXER_SHOWMIXERPANEL" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::ShowMixerPanel,
        })),
        "ZT_SHOWMIXER_DROPDOWNBACKGROUNDACTIVATED" => {
            Ok(UiActionRecord::Show(UiShowActionRecord {
                trigger,
                action: UiShowAction::DismissMixerDropdown,
            }))
        }
        "ZT_SHOWMIXER_SETSHOWOPEN" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::SetSelectedOpen {
                open: event
                    .value
                    .as_deref()
                    .and_then(parse_bool)
                    .ok_or_else(|| invalid_at(input, "show-open action requires a bool"))?,
            },
        })),
        "ZT_SHOWMIXER_TOGGLEEDITSHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::ToggleEditing,
        })),
        "ZT_SHOWMIXER_CANCELEDIT" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::CancelEdit,
        })),
        "ZT_SHOWSCHEDULER_REQUEST_ADDSHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::RequestAddShow,
        })),
        "ZT_SHOWSCHEDULER_ADDSHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::AddShow,
        })),
        "ZT_SHOWSCHEDULER_REQUEST_DELETESHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::RequestDeleteSelected,
        })),
        "ZT_SHOWSCHEDULER_DELETESHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::DeleteSelected,
        })),
        "ZT_SHOWSCHEDULER_VIEWSHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::ViewSelected,
        })),
        "ZT_SHOWSCHEDULER_EDITSHOW" | "ZT_SHOWSCHEDULER_SCHEDULERTABCLICKED" => {
            Ok(UiActionRecord::Show(UiShowActionRecord {
                trigger,
                action: UiShowAction::EditSelected,
            }))
        }
        "ZT_SHOWSCHEDULER_ADDBREAK" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::AddBreak,
        })),
        "ZT_SHOWSCHEDULER_MOVEUP" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::MoveSelectedUp,
        })),
        "ZT_SHOWSCHEDULER_MOVEDOWN" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::MoveSelectedDown,
        })),
        "ZT_SHOWPLATFORM_TRANSACTCURRENTUPGRADE" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::PurchaseSelectedPlatformUpgrade,
        })),
        "ZT_SHOWMIXER_REQUEST_TOGGLEEDITSHOW" => Ok(UiActionRecord::Show(UiShowActionRecord {
            trigger,
            action: UiShowAction::ToggleEditing,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

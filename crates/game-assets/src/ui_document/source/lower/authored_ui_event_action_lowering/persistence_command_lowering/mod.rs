use crate::source_document::ui::model::SourceUiEvent;
use crate::ui_document::source::lower::authored_ui_action_argument_lowering::required_u32;
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::persistence::{
    UiPersistenceAction, UiPersistenceActionRecord,
};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_persistence_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_USER_ATTRIBUTE" => {
            let (key, value) = match (
                event.key.as_deref(),
                event.val.as_deref(),
                event.string.as_deref(),
            ) {
                (Some(key), Some(value), _) => (key, value),
                (None, None, Some(key)) => (key, "true"),
                _ => {
                    return Err(invalid_at(
                        input,
                        "ZT_SET_USER_ATTRIBUTE requires an attribute key or key/value pair",
                    ))
                }
            };
            Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                trigger,
                action: UiPersistenceAction::SetProfileAttribute {
                    key: key.to_owned(),
                    value: value.to_owned(),
                },
            }))
        }
        "ZT_SAVEGAME" | "ZT_LOADGAME" if event.string.as_deref() == Some("cancel") => {
            Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
                trigger,
                action: UiPresentationAction::HideOwningDocument,
            }))
        }
        "ZT_LOADGAME" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
            trigger,
            action: UiPersistenceAction::LoadWorldSnapshotFromSelectedSlot,
        })),
        "ZT_SAVEGAME" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
            trigger,
            action: UiPersistenceAction::SaveWorldSnapshotUnderSubmittedName {
                overwrite_confirmed: event.string.as_deref() == Some("confirmed"),
            },
        })),
        "ZT_LOAD_AFTER_SAVE" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
            trigger,
            action: UiPersistenceAction::OpenLoadSlotCatalogueAfterSaveCompletes,
        })),
        "SAVE" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
            trigger,
            action: UiPersistenceAction::SaveWorldSnapshotToSlot {
                save_slot: required_u32(event, input)?,
            },
        })),
        "LOAD" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
            trigger,
            action: UiPersistenceAction::LoadWorldSnapshotFromSlot {
                save_slot: required_u32(event, input)?,
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

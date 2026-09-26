use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::required_u32;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
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
        "ZT_SAVEGAME" if event.string.as_deref() == Some("cancel") => {
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
            action: UiPersistenceAction::SaveWorldSnapshotToSelectedSlot,
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

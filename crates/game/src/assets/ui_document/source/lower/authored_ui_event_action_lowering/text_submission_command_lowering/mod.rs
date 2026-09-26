use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowActionRecord};
use openzt2_game_data::ui_document::action::information::{
    UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::persistence::{
    UiPersistenceAction, UiPersistenceActionRecord,
};
use openzt2_game_data::ui_document::action::scenarios::{UiScenarioAction, UiScenarioActionRecord};
use openzt2_game_data::ui_document::action::shell_navigation::{
    UiShellAction, UiShellActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use std::io;

pub(super) fn lower_text_submission_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "UI_SEND_TEXT" if event.event_type.as_deref() == Some("ZT_SET_ENTITY_NAME") => {
            Ok(UiActionRecord::Information(UiInformationActionRecord {
                trigger,
                action: UiInformationAction::RenameSelected,
            }))
        }
        "UI_SEND_TEXT" => match event.event_type.as_deref() {
            Some("BFUSERPROFILE_ADD_PROFILE") => {
                Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                    trigger,
                    action: UiPersistenceAction::CreateProfileFromSubmittedDisplayName,
                }))
            }
            Some("BFUSERPROFILE_SET_PROFILE") => {
                Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                    trigger,
                    action: UiPersistenceAction::SelectProfileAtActivatedRow,
                }))
            }
            Some("BFUSERPROFILE_REMOVE_PROFILE") => {
                Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                    trigger,
                    action: UiPersistenceAction::DeleteProfileAtActivatedRow,
                }))
            }
            Some("ZT_DELETEGAME") => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                trigger,
                action: UiPersistenceAction::DeleteWorldSnapshotFromSelectedSlot,
            })),
            Some("ZT_LOADGAME")
                if matches!(role, UiDocumentRole::Globe | UiDocumentRole::MapSelect)
                    && event.target_child.as_deref() == Some("Selected Scenario Map") =>
            {
                Ok(UiActionRecord::Shell(UiShellActionRecord {
                    trigger,
                    action: UiShellAction::StartSelectedWorld,
                }))
            }
            Some("ZT_LOADGAME") => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                trigger,
                action: UiPersistenceAction::LoadWorldSnapshotFromSelectedSlot,
            })),
            Some("ZT_SET_STARTING_SCENARIO_CASH") => {
                Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                    trigger,
                    action: UiScenarioAction::SetStartingCash,
                }))
            }
            Some("ZT_SET_SHOW_NAME") => Ok(UiActionRecord::Show(UiShowActionRecord {
                trigger,
                action: UiShowAction::SetSelectedName,
            })),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    result.map(Some)
}

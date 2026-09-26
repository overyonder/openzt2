use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::information::{
    UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::scenarios::{UiScenarioAction, UiScenarioActionRecord};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_application_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_RUN_SCRIPT" => match event.string.as_deref().unwrap_or_default() {
            value if value.ends_with(" runQuake") => {
                Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                    trigger,
                    action: UiScenarioAction::TriggerEarthquake,
                }))
            }
            value if value.ends_with(" updateQuake") => {
                Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                    trigger,
                    action: UiScenarioAction::RefreshEarthquake,
                }))
            }
            value if value.ends_with(" dehighlightNewAwards") => {
                Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                    trigger,
                    action: UiScenarioAction::MarkAwardsSeen,
                }))
            }
            value if value.ends_with(" populateDataRoots") => {
                Ok(UiActionRecord::Information(UiInformationActionRecord {
                    trigger,
                    action: UiInformationAction::PopulateEntityEditorDataRoots,
                }))
            }
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    result.map(Some)
}

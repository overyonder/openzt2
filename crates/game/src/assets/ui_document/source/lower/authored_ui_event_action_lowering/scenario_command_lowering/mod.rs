use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::objective_status_filter;
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::scenarios::{UiScenarioAction, UiScenarioActionRecord};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_scenario_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_TYPE_FILTER" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::SetObjectiveFilter {
                filter: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref()),
            },
        })),
        "ZT_APPMSG" => match event.xml_object.as_ref() {
            Some(object) if object.key.as_deref() == Some("challenge") && object.value.as_deref() == Some("accept") => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                trigger,
                action: UiScenarioAction::AcceptChallenge,
            })),
            Some(object) if object.key.as_deref() == Some("challenge") && object.value.as_deref() == Some("decline") => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
                trigger,
                action: UiScenarioAction::DeclineChallenge,
            })),
            _ => return Ok(None),
        },
        "ZT_INCREMENT_DISEASE_HINT" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::IncrementDiseaseHint,
        })),
        "ZT_POPULATE_SCENARIO_UI" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::PopulateScenarioSelection,
        })),
        "ZT_PLAY_NEXT_SCENARIO" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::PlayNextScenario,
        })),
        "ZT_SET_STATUS_FILTER" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::SetObjectiveStatusFilter {
                filter: objective_status_filter(event.string.as_deref().or(event.value.as_deref()), input)?,
            },
        })),
        "ZT_SCENARIO_SELECTION_CHANGED" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::SelectionChanged,
        })),
        "ZT_POPULATE_CAMPAIGN_UI" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::PopulateCampaignSelection,
        })),
        "BFS_CLEARSCENARIO" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::ClearSelection,
        })),
        "ZT_PLAY_TUTORIAL" => Ok(UiActionRecord::Scenario(UiScenarioActionRecord {
            trigger,
            action: UiScenarioAction::PlayTutorial,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

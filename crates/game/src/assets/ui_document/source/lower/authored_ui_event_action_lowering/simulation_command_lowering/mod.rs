use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use openzt2_game_data::ui_document::action::simulation_time::{
    UiSimulationAction, UiSimulationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_simulation_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_PAUSE" => Ok(UiActionRecord::Simulation(UiSimulationActionRecord {
            trigger,
            action: UiSimulationAction::SetSimulationPaused {
                paused: match event.string.as_deref() {
                    Some("pause") => true,
                    Some("unpause") => false,
                    Some(value) => {
                        return Err(invalid_at(
                            input,
                            format!("unsupported ZT_PAUSE state {value:?}"),
                        ));
                    }
                    None => event
                        .value
                        .as_deref()
                        .or(event.data.as_deref())
                        .and_then(parse_bool)
                        .unwrap_or(true),
                },
            },
        })),
        "ZT_PAUSE_KEY" => Ok(UiActionRecord::Simulation(UiSimulationActionRecord {
            trigger,
            action: UiSimulationAction::ToggleSimulationPaused,
        })),
        "ZT_TEMPORARY_PAUSE" => Ok(UiActionRecord::Simulation(UiSimulationActionRecord {
            trigger,
            action: UiSimulationAction::SetSimulationPaused {
                paused: event
                    .value
                    .as_deref()
                    .or(event.data.as_deref())
                    .and_then(parse_bool)
                    .unwrap_or(true),
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

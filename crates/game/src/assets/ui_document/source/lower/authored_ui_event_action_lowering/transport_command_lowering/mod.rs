use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::parse_bool;
use openzt2_game_data::ui_document::action::transportation::{
    UiTransportAction, UiTransportActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_transport_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_GENERATE_VEHICLE" => Ok(UiActionRecord::Transport(UiTransportActionRecord {
            trigger,
            action: UiTransportAction::GenerateVehicleForSelectedTransportCircuit,
        })),
        "ZT_TOGGLE_CIRCUIT_STATUS" => Ok(UiActionRecord::Transport(UiTransportActionRecord {
            trigger,
            action: UiTransportAction::SetSelectedTransportCircuitOpen {
                open: event.value.as_deref().and_then(parse_bool).unwrap_or(false),
            },
        })),
        "ZT_CHANGE_CIRCUIT_DIRECTION" => Ok(UiActionRecord::Transport(UiTransportActionRecord {
            trigger,
            action: UiTransportAction::ReverseSelectedTransportCircuitDirection,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

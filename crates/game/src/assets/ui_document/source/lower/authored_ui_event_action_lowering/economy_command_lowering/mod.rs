use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    event_i32, price_index, required_i64, required_u8,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use openzt2_game_data::ui_document::action::economy::{UiEconomyAction, UiEconomyActionRecord};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_economy_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_ADMISSION_PRICE_INDEX" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::SetZooAdmissionPriceBand {
                price_band_index: event_i32(event, 0),
            },
        })),
        "ZT_ZOO_OPEN" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::SetZooAdmissionsOpen {
                open: event_i32(event, 0) != 0,
            },
        })),
        "ZT_GIVE_CASH_GRANT" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::GrantZooCash {
                amount: required_i64(event, input)?,
            },
        })),
        "ZT_SET_PRICE_INDEX" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::SetSelectedFacilityPriceIndex {
                price_index: price_index(current, role, input)?,
            },
        })),
        "ZT_SET_FILTER_MAINTENANCE" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::SetSelectedMaintenanceSchedule {
                maintenance_schedule_index: required_u8(event, input)?,
            },
        })),
        "ZT_UPDATE_SELL_MESSAGE" => Ok(UiActionRecord::Economy(UiEconomyActionRecord {
            trigger,
            action: UiEconomyAction::RefreshSelectedEntitySellQuote,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

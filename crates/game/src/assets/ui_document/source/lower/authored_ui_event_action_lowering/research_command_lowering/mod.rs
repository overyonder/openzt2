use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::research::UiStartResearchForSelectedCatalogueItemActionRecord;
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_research_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_START_RESEARCHING_ENTITY" => Ok(UiActionRecord::StartResearchForSelectedCatalogueItem(
            UiStartResearchForSelectedCatalogueItemActionRecord { trigger },
        )),
        _ => return Ok(None),
    };
    result.map(Some)
}

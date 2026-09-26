use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::animal_health::{
    UiAnimalHealthAction, UiAnimalHealthActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_animal_health_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_ENTER_DISEASE_MODE" => Ok(UiActionRecord::AnimalHealth(UiAnimalHealthActionRecord {
            trigger,
            action: UiAnimalHealthAction::EnterDiseaseTreatmentMode,
        })),
        "ZT_ENTER_TRANQ_MODE" => Ok(UiActionRecord::AnimalHealth(UiAnimalHealthActionRecord {
            trigger,
            action: UiAnimalHealthAction::EnterTranquilizerMode,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

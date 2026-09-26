use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::immersive_mode::{
    UiEnterImmersiveModeActionRecord, UiImmersiveModeKind,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_immersive_mode_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_BOARD_SELECTED_ENTITY" => Ok(UiActionRecord::EnterImmersiveMode(
            UiEnterImmersiveModeActionRecord {
                trigger,
                mode: UiImmersiveModeKind::SelectedEntityFirstPerson,
            },
        )),
        "ZT_ENTER_CLONING_MODE" => Ok(UiActionRecord::EnterImmersiveMode(
            UiEnterImmersiveModeActionRecord {
                trigger,
                mode: UiImmersiveModeKind::Cloning,
            },
        )),
        "ZT_ENTER_FOSSIL_MODE" => Ok(UiActionRecord::EnterImmersiveMode(
            UiEnterImmersiveModeActionRecord {
                trigger,
                mode: UiImmersiveModeKind::FossilSearch,
            },
        )),
        "ZT_ENTER_PUZZLE_MODE" => Ok(UiActionRecord::EnterImmersiveMode(
            UiEnterImmersiveModeActionRecord {
                trigger,
                mode: UiImmersiveModeKind::FossilAssembly,
            },
        )),
        _ => return Ok(None),
    };
    result.map(Some)
}

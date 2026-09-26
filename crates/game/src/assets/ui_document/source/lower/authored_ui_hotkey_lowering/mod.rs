use crate::assets::source_document::ui::model::{SourceUiHotkeyTrigger, SourceUiNode};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_action_lowering;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::hotkey::{
    UiDocumentHotkeyControlState, UiDocumentHotkeyDefinition, UiDocumentHotkeyTrigger,
};
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn add_lowered_hotkey_definitions_to_current_ui_node(
    node: &SourceUiNode,
    node_id_value: AssetId,
    role: UiDocumentRole,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<()> {
    for hotkey in &node.hotkeys {
        let Some(action) =
            authored_ui_event_action_lowering::lower_authored_ui_event_to_canonical_action(
                UiTrigger::Press,
                &hotkey.event,
                role,
                node_id_value,
                node.name.as_deref().unwrap_or_default(),
                node_id_value,
                output,
                input,
                None,
            )?
        else {
            continue;
        };
        let character = hotkey.character.as_deref().unwrap_or_default().to_owned();
        output.hotkeys.push(UiDocumentHotkeyDefinition {
            trigger: match hotkey.trigger {
                SourceUiHotkeyTrigger::Down => UiDocumentHotkeyTrigger::KeyPressed,
                SourceUiHotkeyTrigger::Up => UiDocumentHotkeyTrigger::KeyReleased,
                SourceUiHotkeyTrigger::Unknown(ref value) => {
                    return Err(invalid_at(
                        input,
                        format!("unknown hotkey trigger {value:?}"),
                    ));
                }
            },
            key_code: hotkey.code.unwrap_or(0),
            character,
            control_state: control_state(hotkey.control_state.as_deref(), input)?,
            allow_repeat: hotkey.allow_repeat.unwrap_or(false),
            action,
        });
    }
    Ok(())
}

pub(super) fn control_state(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<UiDocumentHotkeyControlState> {
    match value.unwrap_or("0") {
        "-1" => Ok(UiDocumentHotkeyControlState::AnyControlState),
        "0" => Ok(UiDocumentHotkeyControlState::ControlReleased),
        "1" => Ok(UiDocumentHotkeyControlState::ControlPressed),
        unknown => Err(invalid_at(
            input,
            format!("unknown hotkey control state {unknown:?}"),
        )),
    }
}

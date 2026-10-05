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
        // Keys are matched without Shift, so `<char key="c">` and `key="C">` are one binding.
        let character = hotkey
            .character
            .as_deref()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let authored_control_state = control_state(hotkey.control_state.as_deref(), input)?;
        let (key_code, control_state) = match hotkey.trigger {
            SourceUiHotkeyTrigger::Character => {
                character_code_virtual_key(hotkey.code, authored_control_state)
            }
            _ => (hotkey.code.unwrap_or(0), authored_control_state),
        };
        let definition = UiDocumentHotkeyDefinition {
            trigger: match hotkey.trigger {
                SourceUiHotkeyTrigger::Down | SourceUiHotkeyTrigger::Character => {
                    UiDocumentHotkeyTrigger::KeyPressed
                }
                SourceUiHotkeyTrigger::Up => UiDocumentHotkeyTrigger::KeyReleased,
                SourceUiHotkeyTrigger::Unknown(ref value) => {
                    return Err(invalid_at(
                        input,
                        format!("unknown hotkey trigger {value:?}"),
                    ));
                }
            },
            key_code,
            character,
            control_state,
            allow_repeat: hotkey.allow_repeat.unwrap_or(false),
            action,
        };
        if !output.hotkeys.contains(&definition) {
            output.hotkeys.push(definition);
        }
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

/// Maps a `<char>` character code to the virtual key that types it. Control
/// characters 1-26 are Ctrl+A to Ctrl+Z, except Backspace, Tab and Enter when
/// the binding does not require Ctrl. A `<char key>` binding has no code.
fn character_code_virtual_key(
    code: Option<u32>,
    control_state: UiDocumentHotkeyControlState,
) -> (u32, UiDocumentHotkeyControlState) {
    match code {
        None => (0, control_state),
        Some(0x08 | 0x09 | 0x0d)
            if control_state != UiDocumentHotkeyControlState::ControlPressed =>
        {
            (code.unwrap_or(0), control_state)
        }
        Some(control @ 0x01..=0x1a) => (
            u32::from(b'A') - 1 + control,
            UiDocumentHotkeyControlState::ControlPressed,
        ),
        Some(lowercase @ 0x61..=0x7a) => (lowercase - 0x20, control_state),
        Some(code) => (code, control_state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        ui::parser::SourceUiDocument,
    };
    use crate::assets::ui_document::source::lower::authored_ui_document_lowering::lower_ui_document;
    use openzt2_game_data::ui_document::action::{
        presentation::{UiPresentationAction, UiPresentationActionRecord},
        shell_navigation::{UiShellAction, UiShellActionRecord},
        UiActionRecord,
    };

    #[test]
    fn overhead_mode_shortcuts_lower_to_their_keys_and_actions() {
        let parsed = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/shell.xml"),
            br#"<UILayout name="MainGUI"><UIRegion w="1024" h="768"/><UIHotKeys>
                <char code="13" ctrlState="1" msg="ZT_ACTIVATE_UI_BUTTON" data="BFString" string="animals"/>
                <down code="27" msg="ZT_ESCAPE_KEY"/>
                <char key="c" msg="ZT_UNDOACTION"/>
                <char key="C" msg="ZT_UNDOACTION"/>
            </UIHotKeys><children><UIButton name="animals"/></children></UILayout>"#,
        )
        .unwrap();
        let document = lower_ui_document(&AuthoredUiDocument {
            virtual_path: "ui/layout/shell.xml".into(),
            id: AssetId::default(),
            role: UiDocumentRole::InGameHud,
            source: SourceUiDocument::parse(&parsed),
            templates: Default::default(),
            image_selections: Vec::new(),
            interaction_cursors: None,
            placement_preview: None,
            role_targets: Default::default(),
            resolved_dependencies: Default::default(),
        })
        .unwrap();
        let hotkeys = &document.nodes[0].hotkeys;
        assert_eq!(hotkeys.len(), 3, "c and C are one binding: {hotkeys:?}");
        assert_eq!(
            (
                hotkeys[0].key_code,
                hotkeys[0].control_state,
                &hotkeys[0].action
            ),
            (
                0x4d,
                UiDocumentHotkeyControlState::ControlPressed,
                &UiActionRecord::Presentation(UiPresentationActionRecord {
                    trigger: UiTrigger::Press,
                    action: UiPresentationAction::ActivateTargetNodeWithPress {
                        target_node: UiDocumentRole::InGameHud.node_id("animals"),
                    },
                }),
            )
        );
        assert_eq!(
            (hotkeys[1].key_code, &hotkeys[1].action),
            (
                27,
                &UiActionRecord::Shell(UiShellActionRecord {
                    trigger: UiTrigger::Press,
                    action: UiShellAction::OpenInGameOptionsFromOverheadEscape,
                }),
            )
        );
        assert_eq!(hotkeys[2].character, "C");
    }

    #[test]
    fn character_codes_map_to_the_keys_that_type_them() {
        use UiDocumentHotkeyControlState::{AnyControlState, ControlPressed, ControlReleased};
        // Ctrl-M, Ctrl-R without an authored control state, Tab, and a digit.
        assert_eq!(
            character_code_virtual_key(Some(13), ControlPressed),
            (0x4d, ControlPressed)
        );
        assert_eq!(
            character_code_virtual_key(Some(18), ControlReleased),
            (0x52, ControlPressed)
        );
        assert_eq!(
            character_code_virtual_key(Some(9), AnyControlState),
            (0x09, AnyControlState)
        );
        assert_eq!(
            character_code_virtual_key(Some(u32::from(b'1')), ControlReleased),
            (0x31, ControlReleased)
        );
        assert_eq!(
            character_code_virtual_key(Some(u32::from(b'c')), ControlReleased),
            (0x43, ControlReleased)
        );
        assert_eq!(
            character_code_virtual_key(None, ControlReleased),
            (0, ControlReleased)
        );
    }
}

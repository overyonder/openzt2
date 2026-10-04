use bevy::prelude::*;
use openzt2_game_data::ui_document::{action::UiActionRecord, hotkey::UiDocumentHotkeyTrigger};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::ui::authored_hotkey_keyboard_activation::UiAuthoredHotkeyKeyboardActivationBinding;
use crate::plugins::ui::authored_ui_action_projection_components::{
    UiActionSource, UiAnimalActions, UiAnimalHealthActions, UiAudioSettingActions, UiCameraActions,
    UiConstructionActions, UiEconomyActions, UiImmersiveModeActions, UiInformationActions,
    UiPersistenceActions, UiPhotoActions, UiPopulateCatalogueTypeListActions,
    UiPresentationActions, UiResearchActions, UiScenarioActions, UiShellActions, UiShowActions,
    UiSimulationActions, UiStaffActions, UiTransportActions,
};
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;

pub(super) fn project_authored_hotkey_action_proxies(
    commands: &mut Commands,
    root: Entity,
    document: &UiDocumentAsset,
) {
    for (node_index, node) in document.canonical_ui_document().nodes.iter().enumerate() {
        for (hotkey_index, hotkey) in node.hotkeys.iter().enumerate() {
            let Some(key) = authored_hotkey_key_code(hotkey.key_code, &hotkey.character) else {
                continue;
            };
            let proxy = commands
                .spawn((
                    Name::new("ui hotkey"),
                    UiAuthoredHotkeyKeyboardActivationBinding::from_projected_authored_hotkey(
                        node_index as u32,
                        key,
                        hotkey.control_state,
                        matches!(&hotkey.trigger, UiDocumentHotkeyTrigger::KeyReleased),
                        hotkey.allow_repeat,
                    ),
                    UiDocumentOwner(root),
                    ChildOf(root),
                ))
                .id();
            let source = UiActionSource::Hotkey {
                node: node_index as u32,
                hotkey: hotkey_index as u32,
            };
            match &hotkey.action {
                UiActionRecord::Presentation(_) => {
                    commands.entity(proxy).insert(UiPresentationActions(source));
                }
                UiActionRecord::Shell(_) => {
                    commands.entity(proxy).insert(UiShellActions(source));
                }
                UiActionRecord::PopulateCatalogueTypeList(_) => {
                    commands
                        .entity(proxy)
                        .insert(UiPopulateCatalogueTypeListActions(source));
                }
                UiActionRecord::Construction(_) => {
                    commands.entity(proxy).insert(UiConstructionActions(source));
                }
                UiActionRecord::Camera(_) => {
                    commands.entity(proxy).insert(UiCameraActions(source));
                }
                UiActionRecord::Photo(_) => {
                    commands.entity(proxy).insert(UiPhotoActions(source));
                }
                UiActionRecord::StartResearchForSelectedCatalogueItem(_) => {
                    commands.entity(proxy).insert(UiResearchActions(source));
                }
                UiActionRecord::AnimalHealth(_) => {
                    commands.entity(proxy).insert(UiAnimalHealthActions(source));
                }
                UiActionRecord::Show(_) => {
                    commands.entity(proxy).insert(UiShowActions(source));
                }
                UiActionRecord::Transport(_) => {
                    commands.entity(proxy).insert(UiTransportActions(source));
                }
                UiActionRecord::Scenario(_) => {
                    commands.entity(proxy).insert(UiScenarioActions(source));
                }
                UiActionRecord::AudioSetting(_) => {
                    commands.entity(proxy).insert(UiAudioSettingActions(source));
                }
                UiActionRecord::Information(_) => {
                    commands.entity(proxy).insert(UiInformationActions(source));
                }
                UiActionRecord::EnterImmersiveMode(_) => {
                    commands
                        .entity(proxy)
                        .insert(UiImmersiveModeActions(source));
                }
                UiActionRecord::Staff(_) => {
                    commands.entity(proxy).insert(UiStaffActions(source));
                }
                UiActionRecord::Economy(_) => {
                    commands.entity(proxy).insert(UiEconomyActions(source));
                }
                UiActionRecord::Animal(_) => {
                    commands.entity(proxy).insert(UiAnimalActions(source));
                }
                UiActionRecord::Persistence(_) => {
                    commands.entity(proxy).insert(UiPersistenceActions(source));
                }
                UiActionRecord::Simulation(_) => {
                    commands.entity(proxy).insert(UiSimulationActions(source));
                }
            };
        }
    }
}

fn authored_hotkey_key_code(virtual_key_code: u32, character: &str) -> Option<KeyCode> {
    character
        .chars()
        .next()
        .and_then(authored_hotkey_character_key_code)
        .or_else(|| windows_virtual_key_code(virtual_key_code))
}

fn authored_hotkey_character_key_code(value: char) -> Option<KeyCode> {
    match value.to_ascii_uppercase() {
        'A'..='Z' => windows_virtual_key_code(value.to_ascii_uppercase() as u32),
        '0'..='9' => windows_virtual_key_code(value as u32),
        ' ' => Some(KeyCode::Space),
        _ => None,
    }
}

fn windows_virtual_key_code(code: u32) -> Option<KeyCode> {
    Some(match code {
        0x08 => KeyCode::Backspace,
        0x09 => KeyCode::Tab,
        0x0d => KeyCode::Enter,
        0x1b => KeyCode::Escape,
        0x20 => KeyCode::Space,
        0x25 => KeyCode::ArrowLeft,
        0x26 => KeyCode::ArrowUp,
        0x27 => KeyCode::ArrowRight,
        0x28 => KeyCode::ArrowDown,
        0x30 => KeyCode::Digit0,
        0x31 => KeyCode::Digit1,
        0x32 => KeyCode::Digit2,
        0x33 => KeyCode::Digit3,
        0x34 => KeyCode::Digit4,
        0x35 => KeyCode::Digit5,
        0x36 => KeyCode::Digit6,
        0x37 => KeyCode::Digit7,
        0x38 => KeyCode::Digit8,
        0x39 => KeyCode::Digit9,
        0x41 => KeyCode::KeyA,
        0x42 => KeyCode::KeyB,
        0x43 => KeyCode::KeyC,
        0x44 => KeyCode::KeyD,
        0x45 => KeyCode::KeyE,
        0x46 => KeyCode::KeyF,
        0x47 => KeyCode::KeyG,
        0x48 => KeyCode::KeyH,
        0x49 => KeyCode::KeyI,
        0x4a => KeyCode::KeyJ,
        0x4b => KeyCode::KeyK,
        0x4c => KeyCode::KeyL,
        0x4d => KeyCode::KeyM,
        0x4e => KeyCode::KeyN,
        0x4f => KeyCode::KeyO,
        0x50 => KeyCode::KeyP,
        0x51 => KeyCode::KeyQ,
        0x52 => KeyCode::KeyR,
        0x53 => KeyCode::KeyS,
        0x54 => KeyCode::KeyT,
        0x55 => KeyCode::KeyU,
        0x56 => KeyCode::KeyV,
        0x57 => KeyCode::KeyW,
        0x58 => KeyCode::KeyX,
        0x59 => KeyCode::KeyY,
        0x5a => KeyCode::KeyZ,
        0x70..=0x7b => [
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
        ][(code - 0x70) as usize],
        _ => return None,
    })
}

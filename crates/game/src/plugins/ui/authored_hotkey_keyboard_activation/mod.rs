use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::hotkey::UiDocumentHotkeyControlState;

use crate::plugins::input::input_types::ActionSource;
use crate::plugins::input::{
    input_types::{GameAction, GameActionInputBindings, InputChord},
    keyboard_and_mouse_input_translation::input_chord_was_just_pressed_by_keyboard_or_mouse,
};

use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AuthoredHotkeyGameplayInputOverlap<'w, 's> {
    hotkeys: Query<'w, 's, &'static UiAuthoredHotkeyKeyboardActivationBinding>,
    bindings: Res<'w, GameActionInputBindings>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
}

impl AuthoredHotkeyGameplayInputOverlap<'_, '_> {
    pub(crate) fn uses_just_translated_game_action_chord(
        &self,
        node: Entity,
        action: GameAction,
    ) -> bool {
        let Ok(hotkey) = self.hotkeys.get(node) else {
            return false;
        };
        self.bindings
            .entries
            .iter()
            .filter(|binding| binding.action == action)
            .flat_map(|binding| std::iter::once(binding.primary).chain(binding.alternate))
            .any(|chord| {
                let contains_key = match chord {
                    InputChord::Key(key) | InputChord::ModifiedKey { key, .. } => key == hotkey.key,
                    InputChord::KeyPair(first, second) => {
                        first == hotkey.key || second == hotkey.key
                    }
                    _ => false,
                };
                contains_key
                    && input_chord_was_just_pressed_by_keyboard_or_mouse(
                        chord,
                        &self.keys,
                        &self.buttons,
                    )
            })
    }
}

/// A hotkey proxy. It names its receiver by document preorder index and
/// deliberately has no `UiNodeId`, so node lookups never mistake it for the
/// receiver itself.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredHotkeyKeyboardActivationBinding {
    receiver_index: u32,
    key: KeyCode,
    control_state: UiDocumentHotkeyControlState,
    activates_on_release: bool,
    allows_repeated_input: bool,
}

impl UiAuthoredHotkeyKeyboardActivationBinding {
    pub(super) fn is_cancel_activation(&self) -> bool {
        self.key == KeyCode::Escape
            && !self.activates_on_release
            && self.control_state != UiDocumentHotkeyControlState::ControlPressed
    }
    pub(super) fn receiver_index(&self) -> u32 {
        self.receiver_index
    }
    pub(super) fn from_projected_authored_hotkey(
        receiver_index: u32,
        key: KeyCode,
        control_state: UiDocumentHotkeyControlState,
        activates_on_release: bool,
        allows_repeated_input: bool,
    ) -> Self {
        Self {
            receiver_index,
            key,
            control_state,
            activates_on_release,
            allows_repeated_input,
        }
    }
}

pub(super) fn activate_projected_authored_hotkeys_from_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard: MessageReader<KeyboardInput>,
    bindings: Query<(
        Entity,
        &UiAuthoredHotkeyKeyboardActivationBinding,
        &super::authored_ui_node_projection_components::UiDocumentOwner,
    )>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut activated: MessageWriter<UiNodeActivated>,
    focus: Res<InputFocus>,
    text_edits: Query<(), With<EditableText>>,
) {
    let active_modal = context.top_modal();
    let control_is_pressed =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for input in keyboard.read() {
        if focus
            .get()
            .is_some_and(|focused| text_edits.contains(focused))
            && !matches!(
                input.key_code,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape | KeyCode::Tab
            )
        {
            continue;
        }
        for (entity, binding, owner) in &bindings {
            // Escape dismisses one activity. The cancellation receiver selection
            // chooses it for keyboard and controller alike.
            if binding.is_cancel_activation()
                || !context.hotkey_receiver_is_eligible(
                    owner.0,
                    binding.receiver_index(),
                    None,
                    active_modal,
                )
            {
                continue;
            }
            let control_state_matches = match binding.control_state {
                UiDocumentHotkeyControlState::ControlReleased => !control_is_pressed,
                UiDocumentHotkeyControlState::ControlPressed => control_is_pressed,
                UiDocumentHotkeyControlState::AnyControlState => true,
            };
            let input_edge_matches =
                matches!(input.state, ButtonState::Released) == binding.activates_on_release;
            if input.key_code == binding.key
                && control_state_matches
                && input_edge_matches
                && (binding.allows_repeated_input || !input.repeat)
            {
                activated.write(UiNodeActivated {
                    source: ActionSource::KeyboardMouse,
                    node: entity,
                    trigger: UiTrigger::Press,
                });
            }
        }
    }
}

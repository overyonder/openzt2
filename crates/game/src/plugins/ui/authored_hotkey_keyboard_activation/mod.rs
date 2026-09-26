use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::hotkey::UiDocumentHotkeyControlState;

use crate::plugins::input::input_types::ActionSource;

use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredHotkeyKeyboardActivationBinding {
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
    pub(super) fn from_projected_authored_hotkey(
        key: KeyCode,
        control_state: UiDocumentHotkeyControlState,
        activates_on_release: bool,
        allows_repeated_input: bool,
    ) -> Self {
        Self {
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
        &super::authored_ui_node_projection_components::UiNodeId,
    )>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut activated: MessageWriter<UiNodeActivated>,
    focus: Res<InputFocus>,
    text_edits: Query<(), With<EditableText>>,
) {
    let active_scope = context.active_scope();
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
        for (entity, binding, owner, id) in &bindings {
            if !context.hotkey_receiver_is_eligible(
                owner.0,
                *id,
                binding
                    .is_cancel_activation()
                    .then_some(active_scope)
                    .flatten(),
                active_modal,
            ) {
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

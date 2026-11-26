use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::hotkey::UiDocumentHotkeyControlState;

use openzt2_interface_state::input::input_chord_queries::input_chord_was_just_pressed_by_keyboard_or_mouse;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::input::input_types::GameActionInputBindings;
use openzt2_interface_state::input::input_types::InputChord;

use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(bevy::ecs::system::SystemParam)]
pub struct AuthoredHotkeyGameplayInputOverlap<'w, 's> {
    hotkeys: Query<'w, 's, &'static UiAuthoredHotkeyKeyboardActivationBinding>,
    bindings: Res<'w, GameActionInputBindings>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
}

impl AuthoredHotkeyGameplayInputOverlap<'_, '_> {
    pub fn uses_just_translated_game_action_chord(&self, node: Entity, action: GameAction) -> bool {
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

/// A hotkey proxy parented to its receiver.
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
    bindings: Query<(Entity, &UiAuthoredHotkeyKeyboardActivationBinding, &ChildOf)>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut activated: MessageWriter<UiNodeActivated>,
    focus: Res<InputFocus>,
    text_edits: Query<(), With<EditableText>>,
) {
    let mut active_modal = None;
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
        for (entity, binding, receiver) in &bindings {
            // Escape dismisses one activity. The cancellation receiver selection
            // chooses it for keyboard and controller alike.
            if binding.key != input.key_code || binding.is_cancel_activation() {
                continue;
            }
            let control_state_matches = match binding.control_state {
                UiDocumentHotkeyControlState::ControlReleased => !control_is_pressed,
                UiDocumentHotkeyControlState::ControlPressed => control_is_pressed,
                UiDocumentHotkeyControlState::AnyControlState => true,
            };
            let input_edge_matches =
                matches!(input.state, ButtonState::Released) == binding.activates_on_release;
            if control_state_matches
                && input_edge_matches
                && (binding.allows_repeated_input || !input.repeat)
                && context.visible_and_enabled(receiver.parent())
                && active_modal
                    .get_or_insert_with(|| context.top_modal())
                    .is_none_or(|modal| context.is_in_modal(receiver.parent(), modal))
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

/// Resolved authored press bindings with the state of their physical keys.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AuthoredHeldCameraCommands<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    documents: Res<'w, Assets<openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset>>,
    roots: Query<'w, 's, &'static openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot>,
    bindings: Query<'w, 's, (
        &'static UiAuthoredHotkeyKeyboardActivationBinding,
        &'static ChildOf,
        &'static openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner,
        &'static openzt2_interface_state::ui::authored_ui_action_projection_components::UiCameraActions,
    )>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext<'w, 's>,
    focus: Res<'w, InputFocus>,
    edits: Query<'w, 's, (), With<EditableText>>,
}

impl AuthoredHeldCameraCommands<'_, '_> {
    pub fn for_role(
        &self,
        role: openzt2_game_data::ui_document::document::UiDocumentRole,
    ) -> impl Iterator<Item = openzt2_game_data::AssetId> + '_ {
        use openzt2_game_data::ui_document::action::camera::UiCameraAction;
        let control =
            self.keys.pressed(KeyCode::ControlLeft) || self.keys.pressed(KeyCode::ControlRight);
        let editing = self
            .focus
            .get()
            .is_some_and(|focus| self.edits.contains(focus));
        let modal = self.context.top_modal();
        self.bindings
            .iter()
            .filter_map(move |(binding, parent, owner, actions)| {
                if editing
                    || binding.activates_on_release
                    || !(self.keys.pressed(binding.key)
                        || (binding.key == KeyCode::ControlLeft && control))
                    || !matches!(
                        (binding.control_state, control),
                        (UiDocumentHotkeyControlState::AnyControlState, _)
                            | (UiDocumentHotkeyControlState::ControlReleased, false)
                            | (UiDocumentHotkeyControlState::ControlPressed, true)
                    ) && binding.key != KeyCode::ControlLeft
                    || !self.context.visible_and_enabled(parent.parent())
                    || modal.is_some_and(|modal| !self.context.is_in_modal(parent.parent(), modal))
                {
                    return None;
                }
                let root = self.roots.get(owner.0).ok()?;
                let document = self.documents.get(&root.document)?;
                (document.canonical_ui_document().role == role).then_some((actions, document))
            })
            .flat_map(|(actions, document)| actions.authored_action_records(document))
            .filter_map(|record| match record.action {
                UiCameraAction::SetCommandState {
                    command,
                    pressed: true,
                } => Some(command),
                _ => None,
            })
    }
}

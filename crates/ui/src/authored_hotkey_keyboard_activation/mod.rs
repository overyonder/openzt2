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
    either_control_key: bool,
}

impl UiAuthoredHotkeyKeyboardActivationBinding {
    fn matches_key(&self, key: KeyCode) -> bool {
        self.key == key || (self.either_control_key && key == KeyCode::ControlRight)
    }

    fn is_held(&self, keys: &ButtonInput<KeyCode>, control: bool) -> bool {
        let selects_control_key = matches!(self.key, KeyCode::ControlLeft | KeyCode::ControlRight);
        !self.activates_on_release
            && (keys.pressed(self.key) || self.either_control_key && control)
            && (selects_control_key || self.modifiers_match(control))
    }

    fn modifiers_match(&self, control: bool) -> bool {
        match self.control_state {
            UiDocumentHotkeyControlState::ControlReleased => !control,
            UiDocumentHotkeyControlState::ControlPressed => control,
            UiDocumentHotkeyControlState::AnyControlState => true,
        }
    }

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
        either_control_key: bool,
    ) -> Self {
        Self {
            key,
            control_state,
            activates_on_release,
            allows_repeated_input,
            either_control_key,
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
        let chosen = bindings
            .iter()
            .filter_map(|(entity, binding, receiver)| {
                // Escape dismisses one activity. The cancellation receiver selection
                // chooses it for keyboard and controller alike.
                if !binding.matches_key(input.key_code) || binding.is_cancel_activation() {
                    return None;
                }
                let control_state_matches =
                    matches!(binding.key, KeyCode::ControlLeft | KeyCode::ControlRight)
                        || binding.modifiers_match(control_is_pressed);
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
                    return Some((context.navigation_order(receiver.parent()), entity));
                }
                None
            })
            .max_by_key(|(order, entity)| (*order, std::cmp::Reverse(entity.to_bits())));
        if let Some((_, node)) = chosen {
            activated.write(UiNodeActivated {
                source: ActionSource::KeyboardMouse,
                node,
                trigger: UiTrigger::Press,
            });
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
                    || !binding.is_held(&self.keys, control)
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

    pub fn first_person(
        &self,
        mode: openzt2_game_data::AssetId,
    ) -> impl Iterator<Item = openzt2_game_data::ui_document::action::camera::UiFirstPersonCommand> + '_
    {
        use openzt2_game_data::ui_document::{
            action::{camera::UiCameraAction, UiActionRecord},
            document::UiDocumentRole,
            hotkey::UiDocumentHotkeyTrigger,
        };
        let control =
            self.keys.pressed(KeyCode::ControlLeft) || self.keys.pressed(KeyCode::ControlRight);
        let accepts_input = self.context.top_modal().is_none()
            && !self
                .focus
                .get()
                .is_some_and(|entity| self.edits.contains(entity));
        self.roots
            .iter()
            .filter_map(move |root| {
                let document = self.documents.get(&root.document)?;
                (accepts_input
                    && document.canonical_ui_document().role == UiDocumentRole::InGameHud)
                    .then(|| document.first_person_hotkeys(mode))
            })
            .flatten()
            .filter(move |hotkey| {
                let key = crate::projection::authored_hotkey_projection::authored_hotkey_key_code(
                    hotkey.key_code,
                    &hotkey.character,
                );
                key.is_some_and(|key| {
                    UiAuthoredHotkeyKeyboardActivationBinding::from_projected_authored_hotkey(
                        key,
                        hotkey.control_state,
                        hotkey.trigger == UiDocumentHotkeyTrigger::KeyReleased,
                        hotkey.allow_repeat,
                        hotkey.key_code == 0x11,
                    )
                    .is_held(&self.keys, control)
                })
            })
            .flat_map(|hotkey| &hotkey.actions)
            .filter_map(|action| match action {
                UiActionRecord::Camera(record) => match record.action {
                    UiCameraAction::SetFirstPersonCommandState {
                        command,
                        pressed: true,
                    } => Some(command),
                    _ => None,
                },
                _ => None,
            })
    }
}

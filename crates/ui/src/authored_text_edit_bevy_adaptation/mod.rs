use openzt2_game_data::ui_document::action::UiTrigger;
use std::time::Duration;

use bevy::{
    input_focus::{tab_navigation::TabIndex, InputFocus},
    prelude::*,
    text::{ComputedTextBlock, EditableText, EditableTextFilter, TextCursorStyle},
    ui::widget::TextNodeFlags,
    ui_widgets::{SelectAllOnFocus, TextInput},
};

use openzt2_interface_state::input::input_types::ActionSource;

use super::authored_ui_focus_state::UiFocusable;
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Retains only the authored settings needed after an edit has been lowered
/// into Bevy's canonical `EditableText` representation.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiTextEditPolicy {
    maximum_character_count: u32,
    accepts_only_legal_filename_characters: bool,
    select_all_on_focus: bool,
    cursor_blink_period_seconds: f32,
}

impl UiTextEditPolicy {
    pub(super) fn from_authored_text_edit_definition(
        maximum_character_count: u32,
        accepts_only_legal_filename_characters: bool,
        select_all_on_focus: bool,
        cursor_on_seconds: f32,
        cursor_off_seconds: f32,
    ) -> Self {
        Self {
            maximum_character_count,
            accepts_only_legal_filename_characters,
            select_all_on_focus,
            cursor_blink_period_seconds: cursor_on_seconds + cursor_off_seconds,
        }
    }

    pub const fn accepts_only_legal_filename_characters(self) -> bool {
        self.accepts_only_legal_filename_characters
    }
}

/// Converts the projected static `Text` value into Bevy's native editable text
/// component once. Bevy/Parley then own Unicode navigation, selection, caret
/// geometry, rendering, clipping, scrolling, IME, and clipboard operations.
pub(super) fn initialize_authored_text_edits_as_bevy_editable_text(
    mut commands: Commands,
    mut edits: Query<
        (
            Entity,
            &UiTextEditPolicy,
            Option<&Text>,
            &mut Node,
            &UiNodeId,
            Option<&UiFocusable>,
        ),
        (Added<UiTextEditPolicy>, Without<EditableText>),
    >,
) {
    for (entity, policy, text, mut node, node_id, focusable) in &mut edits {
        let mut editable = EditableText::new(text.map_or("", |text| text.0.as_str()));
        editable.max_characters = (policy.maximum_character_count != 0)
            .then_some(policy.maximum_character_count as usize);
        let blink_period = policy.cursor_blink_period_seconds;
        editable.cursor_blink_period = Duration::from_secs_f32(
            blink_period
                .is_finite()
                .then_some(blink_period)
                .filter(|period| *period > 0.0)
                .unwrap_or(1.0),
        );

        // Bevy's editable viewport keeps the caret inside this content box. Clipping
        // makes the resulting horizontal scroll visible instead of drawing the
        // off-screen portion of the one-line edit over sibling controls.
        node.overflow = Overflow::clip_x();
        let mut entity_commands = commands.entity(entity);
        // Leave Bevy's static-text pipeline entirely. `TextNodeFlags` alone
        // keeps its layout system laying out the projected text's stale block
        // whenever the node changes, overwriting the editor's glyphs, so the
        // value and caret are never drawn.
        entity_commands.remove::<(Text, TextNodeFlags)>().insert((
            ComputedTextBlock::default(),
            editable,
            TextInput,
            TextCursorStyle::default(),
            TextLayout::no_wrap(),
        ));
        // UITextEdit is intrinsically keyboard-focusable. Source documents do
        // not need a separate focus flag, and this must not give every newly
        // projected document an automatically focused text field.
        if focusable.is_none() {
            entity_commands.insert((
                UiFocusable,
                TabIndex(i32::try_from(node_id.index).unwrap_or(i32::MAX)),
            ));
        }
        if policy.select_all_on_focus {
            entity_commands.insert(SelectAllOnFocus);
        }
        if policy.accepts_only_legal_filename_characters {
            entity_commands.insert(EditableTextFilter::new(
                is_legal_authored_filename_character,
            ));
        }
    }
}

/// Bevy's pointer observer can focus an edit directly. Reconcile that path
/// with UI's canonical interaction gate before pending edits are applied.
pub(super) fn discard_pending_edits_and_focus_from_disabled_text_edits(
    mut input_focus: ResMut<InputFocus>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut edits: Query<
        (
            Entity,
            &UiInteractionEnabled,
            &InheritedVisibility,
            &mut EditableText,
        ),
        Or<(
            Changed<UiInteractionEnabled>,
            Changed<InheritedVisibility>,
            Changed<EditableText>,
            Added<bevy::ui::InteractionDisabled>,
        )>,
    >,
) {
    for (entity, enabled, visibility, mut edit) in &mut edits {
        if !enabled.0 || !visibility.get() || !context.enabled(entity) {
            edit.pending_edits.clear();
            edit.pending_paste = None;
            if input_focus.get() == Some(entity) {
                input_focus.clear();
            }
        }
    }
}

/// Bevy emits this observer after applying queued edits. Cursor and selection
/// motions also produce an action, matching the navigation path's
/// value dispatch; canonical consumers can update idempotently.
pub(super) fn dispatch_authored_change_activation_after_bevy_text_edit(
    change: On<bevy::text::TextEditChange>,
    edits: Query<&UiInteractionEnabled, With<UiTextEditPolicy>>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let node = change.event_target();
    if !edits.contains(node) || !context.visible_and_enabled(node) {
        return;
    }
    activated.write(UiNodeActivated {
        source: ActionSource::KeyboardMouse,
        node,
        trigger: UiTrigger::Change,
    });
}

fn is_legal_authored_filename_character(character: char) -> bool {
    !character.is_control()
        && !matches!(
            character,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
        )
}

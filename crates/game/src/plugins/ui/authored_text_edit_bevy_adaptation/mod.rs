use openzt2_game_data::ui_document::action::UiTrigger;
use std::time::Duration;

use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::{EditableText, EditableTextFilter, TextCursorStyle, TextLayoutInfo},
    ui::widget::TextScroll,
    ui_widgets::SelectAllOnFocus,
};

use crate::plugins::input::input_types::ActionSource;

use super::authored_ui_focus_state::{UiFocusPresentation, UiFocusScope, UiFocusable};
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Retains only the authored settings needed after an edit has been lowered
/// into Bevy's canonical `EditableText` representation.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiTextEditPolicy {
    maximum_character_count: u32,
    accepts_only_legal_filename_characters: bool,
    select_all_on_focus: bool,
    cursor_blink_period_seconds: f32,
}

#[derive(Component)]
pub(super) struct UiTextScrollSuppressedDuringScaledCanvasRenderExtraction;

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

    pub(crate) const fn accepts_only_legal_filename_characters(self) -> bool {
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
            &UiInteractionEnabled,
            Option<&UiFocusable>,
        ),
        (Added<UiTextEditPolicy>, Without<EditableText>),
    >,
) {
    for (entity, policy, text, mut node, node_id, enabled, focusable) in &mut edits {
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

        // Bevy's TextScroll keeps the caret inside this content box. Clipping
        // makes the resulting horizontal scroll visible instead of drawing the
        // off-screen portion of the one-line edit over sibling controls.
        node.overflow = Overflow::clip_x();
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<Text>().insert((
            editable,
            TextCursorStyle::default(),
            TextLayout::no_wrap(),
        ));
        // UITextEdit is intrinsically keyboard-focusable. Source documents do
        // not need a separate focus flag, and this must not give every newly
        // projected document an automatically focused text field.
        if focusable.is_none() {
            entity_commands.insert((
                UiFocusable {
                    order: node_id.index,
                    enabled: enabled.0,
                },
                UiFocusPresentation::default(),
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

/// Bevy 0.19's editable-text extractor builds its private self-clip from an
/// unscaled content size around a transformed global centre. On the stretched
/// authored canvas that clip cuts fitting text down to its central suffix.
/// A fitting, unscrolled edit needs no private clip: its authored `Node`
/// already bounds the value. Remove only that redundant render-time marker.
pub(super) fn suppress_fitting_authored_text_edit_scroll_during_render_extraction(
    mut commands: Commands,
    edits: Query<
        (Entity, &TextScroll, &ComputedNode, &TextLayoutInfo),
        (
            With<UiTextEditPolicy>,
            Without<UiTextScrollSuppressedDuringScaledCanvasRenderExtraction>,
        ),
    >,
) {
    for (entity, scroll, node, layout) in &edits {
        if scroll.0 == Vec2::ZERO && layout.size.cmple(node.content_box().size()).all() {
            commands
                .entity(entity)
                .remove::<TextScroll>()
                .insert(UiTextScrollSuppressedDuringScaledCanvasRenderExtraction);
        }
    }
}

/// Restore Bevy's scroll owner before its input, editing and layout systems
/// run on the next frame.
pub(super) fn restore_authored_text_edit_scroll_before_input(
    mut commands: Commands,
    edits: Query<Entity, With<UiTextScrollSuppressedDuringScaledCanvasRenderExtraction>>,
) {
    for entity in &edits {
        commands
            .entity(entity)
            .insert(TextScroll::default())
            .remove::<UiTextScrollSuppressedDuringScaledCanvasRenderExtraction>();
    }
}

/// Bridges the document-local UI focus fact to Bevy's input-focus owner.
/// This is the only focus adaptation needed for Bevy's keyboard, pointer, and
/// IME observers to target the authored edit entity.
pub(super) fn synchronize_document_focus_into_bevy_text_input_focus(
    scopes: Query<(Entity, &UiFocusScope), Changed<UiFocusScope>>,
    edits: Query<
        (
            &UiDocumentOwner,
            &UiInteractionEnabled,
            &InheritedVisibility,
        ),
        With<EditableText>,
    >,
    mut input_focus: ResMut<InputFocus>,
) {
    if input_focus
        .get()
        .and_then(|focused| edits.get(focused).ok())
        .is_some_and(|(_, enabled, visibility)| !enabled.0 || !visibility.get())
    {
        input_focus.clear();
    }
    for (root, scope) in &scopes {
        let next = scope.focused.filter(|focused| {
            edits
                .get(*focused)
                .is_ok_and(|(owner, enabled, visibility)| {
                    owner.0 == root && enabled.0 && visibility.get()
                })
        });
        if let Some(next) = next {
            if input_focus.get() != Some(next) {
                input_focus.set(next, FocusCause::Navigated);
            }
        } else if input_focus
            .get()
            .and_then(|focused| edits.get(focused).ok())
            .is_some_and(|(owner, _, _)| owner.0 == root)
        {
            input_focus.clear();
        }
    }
}

/// Clicking another control or the zoo relinquishes text capture. Keep the
/// authored scope and Bevy's input owner in agreement, including across documents.
pub(super) fn release_text_focus_after_pointer_press_elsewhere(
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    capture: Res<super::picking::UiPointerCapture>,
    edits: Query<(), With<EditableText>>,
    parents: Query<&ChildOf>,
    mut scopes: Query<&mut UiFocusScope>,
    mut input_focus: ResMut<InputFocus>,
) {
    if !primary_pointer.just_pressed {
        return;
    }
    let clicked_edit = capture.target.and_then(|target| {
        std::iter::once(target)
            .chain(parents.iter_ancestors::<ChildOf>(target))
            .find(|entity| edits.contains(*entity))
    });
    for mut scope in &mut scopes {
        if scope.focused.is_some_and(|focused| edits.contains(focused))
            && scope.focused != clicked_edit
        {
            scope.focused = None;
        }
    }
    if input_focus
        .get()
        .is_some_and(|focused| edits.contains(focused))
        && input_focus.get() != clicked_edit
    {
        input_focus.clear();
    }
}

/// Bevy's pointer observer can focus an edit directly. Reconcile that path
/// with UI's canonical interaction gate before pending edits are applied.
pub(super) fn discard_pending_edits_and_focus_from_disabled_text_edits(
    mut input_focus: ResMut<InputFocus>,
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
        )>,
    >,
) {
    for (entity, enabled, visibility, mut edit) in &mut edits {
        if !enabled.0 || !visibility.get() {
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
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let node = change.event_target();
    if !edits.get(node).is_ok_and(|enabled| enabled.0) {
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

use bevy::{
    ecs::entity::EntityHashSet,
    input_focus::{InputFocus, InputFocusVisible},
    picking::{
        events::{PointerCancel, PointerClick, PointerDragEnd, PointerPress, PointerState},
        hover::DirectlyHovered,
        pointer::{PointerButton, PointerId},
    },
    prelude::*,
};
use openzt2_game_data::ui_document::node_presentation::UiNodeVisualState;

use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;

use super::{
    active_authored_ui_context::EffectiveUiInteraction,
    authored_ui_focus_navigation_and_activation::AuthoredHoverPresentation,
    authored_ui_selection_state::UiSelected,
    authored_ui_visual_types::{
        UiVisualImageColor, UiVisualLayer, UiVisualPicking, UiVisualSound, UiVisualTextColor,
    },
};

/// Shows the visual layer for the current interaction state.
pub(super) fn present_authored_ui_interaction_visual_state(
    parents: Query<(
        Entity,
        Ref<DirectlyHovered>,
        Option<Ref<EffectiveUiInteraction>>,
        Option<Ref<AuthoredHoverPresentation>>,
        Option<Ref<UiSelected>>,
        Ref<Children>,
    )>,
    pointer: Res<PointerState>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    (mut presses, mut clicks, mut drag_ends, mut cancels): (
        MessageReader<PointerPress>,
        MessageReader<PointerClick>,
        MessageReader<PointerDragEnd>,
        MessageReader<PointerCancel>,
    ),
    layers: Query<&UiVisualLayer>,
    sounds: Query<&UiVisualSound>,
    colors: Query<&UiVisualTextColor>,
    image_colors: Query<&UiVisualImageColor>,
    picking: Query<&UiVisualPicking>,
    mut visibility: Query<&mut Visibility>,
    mut text_colors: Query<&mut TextColor>,
    mut images: Query<&mut ImageNode>,
    mut parent_picking: Query<&mut Pickable>,
    mut audio: MessageWriter<PlayAudioClip>,
) {
    let mut press_changes = EntityHashSet::default();
    let press = presses
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| event.entity);
    let clicks = clicks
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| event.entity);
    let drag_ends = drag_ends
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| event.entity);
    let cancels = cancels
        .read()
        .filter(|event| event.pointer.id.is_mouse())
        .map(|event| event.entity);
    press_changes.extend(press.chain(clicks).chain(drag_ends).chain(cancels));
    let pressed = pointer.get(PointerId::Mouse, PointerButton::Primary);
    for (entity, hovered, interaction_enabled, hover_presentation, selected, children) in &parents {
        if !hovered.is_changed()
            && !focus.is_changed()
            && !focus_visible.is_changed()
            && !press_changes.contains(&entity)
            && interaction_enabled
                .as_ref()
                .is_none_or(|value| !value.is_changed())
            && hover_presentation
                .as_ref()
                .is_none_or(|value| !value.is_changed())
            && selected.as_ref().is_none_or(|value| !value.is_changed())
            && !children.is_changed()
        {
            continue;
        }
        let alternate = selected.is_some_and(|selected| selected.0);
        let enabled = interaction_enabled.is_none_or(|enabled| enabled.0);
        let is_pressed = pressed.is_some_and(|state| state.pressing.contains_key(&entity));
        let highlighted = hovered.get()
            || hover_presentation.is_some_and(|presentation| presentation.0)
            || (focus_visible.0 && focus.get() == Some(entity));
        let requested = match (enabled, is_pressed, highlighted, alternate) {
            (false, _, _, false) => UiNodeVisualState::Disabled,
            (false, _, _, true) => UiNodeVisualState::AlternateDisabled,
            (true, true, _, false) => UiNodeVisualState::Activated,
            (true, true, _, true) => UiNodeVisualState::AlternateActivated,
            (true, false, true, false) => UiNodeVisualState::Highlighted,
            (true, false, true, true) => UiNodeVisualState::AlternateHighlighted,
            (true, false, false, false) => UiNodeVisualState::Normal,
            (true, false, false, true) => UiNodeVisualState::AlternateNormal,
        };
        let fallback_roles = match requested {
            UiNodeVisualState::Normal => [
                UiNodeVisualState::Normal,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
            ],
            UiNodeVisualState::Highlighted
            | UiNodeVisualState::Activated
            | UiNodeVisualState::Disabled => [
                requested,
                UiNodeVisualState::Normal,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
            ],
            UiNodeVisualState::AlternateNormal => [
                UiNodeVisualState::AlternateNormal,
                UiNodeVisualState::Normal,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
            ],
            UiNodeVisualState::AlternateHighlighted => [
                UiNodeVisualState::AlternateHighlighted,
                UiNodeVisualState::Highlighted,
                UiNodeVisualState::AlternateNormal,
                UiNodeVisualState::Normal,
            ],
            UiNodeVisualState::AlternateActivated => [
                UiNodeVisualState::AlternateActivated,
                UiNodeVisualState::Activated,
                UiNodeVisualState::AlternateNormal,
                UiNodeVisualState::Normal,
            ],
            UiNodeVisualState::AlternateDisabled => [
                UiNodeVisualState::AlternateDisabled,
                UiNodeVisualState::Disabled,
                UiNodeVisualState::AlternateNormal,
                UiNodeVisualState::Normal,
            ],
            UiNodeVisualState::Default => [
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
                UiNodeVisualState::Default,
            ],
        };
        let mut available_roles = [false; 9];
        for child in children.iter() {
            let Ok(layer) = layers.get(child) else {
                continue;
            };
            available_roles[visual_role_index(layer.0)] = true;
        }
        let selected_role = fallback_roles
            .into_iter()
            .find(|role| available_roles[visual_role_index(*role)])
            .or_else(|| {
                available_roles[visual_role_index(UiNodeVisualState::Default)]
                    .then_some(UiNodeVisualState::Default)
            });
        let mut selected_color = None;
        let mut selected_image_color = None;
        let mut selected_picking = None;
        for child in children.iter() {
            let Ok(layer) = layers.get(child) else {
                continue;
            };
            let Ok(mut value) = visibility.get_mut(child) else {
                continue;
            };
            let requested_visibility = if Some(layer.0) == selected_role {
                let entering_state = *value != Visibility::Inherited;
                selected_color = colors.get(child).ok().map(|color| color.0);
                selected_image_color = image_colors.get(child).ok().map(|color| color.0);
                selected_picking = picking.get(child).ok().map(|picking| picking.0);
                if entering_state {
                    if let Ok(sound) = sounds.get(child) {
                        audio.write(PlayAudioClip {
                            clip: sound.0.clone(),
                            emitter: None,
                            selection: child.to_bits(),
                            force_looped: false,
                        });
                    }
                }
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *value != requested_visibility {
                *value = requested_visibility;
            }
        }
        if let (Some(color), Ok(mut text_color)) = (selected_color, text_colors.get_mut(entity)) {
            if text_color.0 != color {
                text_color.0 = color;
            }
        }
        if let (Some(color), Ok(mut image)) = (selected_image_color, images.get_mut(entity)) {
            if image.color != color {
                image.color = color;
            }
        }
        if let (Some(pickable), Ok(mut parent)) = (selected_picking, parent_picking.get_mut(entity))
        {
            let requested = if pickable {
                Pickable::default()
            } else {
                Pickable::IGNORE
            };
            if *parent != requested {
                *parent = requested;
            }
        }
    }
}

const fn visual_role_index(role: UiNodeVisualState) -> usize {
    match role {
        UiNodeVisualState::Default => 0,
        UiNodeVisualState::Normal => 1,
        UiNodeVisualState::Highlighted => 2,
        UiNodeVisualState::Activated => 3,
        UiNodeVisualState::Disabled => 4,
        UiNodeVisualState::AlternateNormal => 5,
        UiNodeVisualState::AlternateHighlighted => 6,
        UiNodeVisualState::AlternateActivated => 7,
        UiNodeVisualState::AlternateDisabled => 8,
    }
}

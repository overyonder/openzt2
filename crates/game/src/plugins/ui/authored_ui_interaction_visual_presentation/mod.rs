use bevy::prelude::*;
use openzt2_game_data::ui_document::node_presentation::UiNodeVisualState;

use crate::plugins::audio::audio_playback_message_types::PlayAudioClip;

use super::{
    authored_ui_focus_state::{UiFocusPresentation, UiFocusable},
    authored_ui_interaction_enabled_state::UiInteractionEnabled,
    authored_ui_selection_state::UiSelected,
    authored_ui_visual_types::{
        UiVisualImageColor, UiVisualLayer, UiVisualPicking, UiVisualSound, UiVisualTextColor,
    },
};

/// Shows the visual layer for the current interaction state.
pub(super) fn present_authored_ui_interaction_visual_state(
    parents: Query<(
        Entity,
        Ref<Interaction>,
        Option<Ref<UiInteractionEnabled>>,
        Option<Ref<UiFocusable>>,
        Option<Ref<UiFocusPresentation>>,
        Option<Ref<UiSelected>>,
        Ref<Children>,
    )>,
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
    for (entity, interaction, interaction_enabled, focusable, focused, selected, children) in
        &parents
    {
        if !interaction.is_changed()
            && interaction_enabled
                .as_ref()
                .is_none_or(|value| !value.is_changed())
            && focusable.as_ref().is_none_or(|value| !value.is_changed())
            && focused.as_ref().is_none_or(|value| !value.is_changed())
            && selected.as_ref().is_none_or(|value| !value.is_changed())
            && !children.is_changed()
        {
            continue;
        }
        let alternate = selected.is_some_and(|selected| selected.0);
        let enabled = interaction_enabled
            .map(|enabled| enabled.0)
            .unwrap_or_else(|| focusable.is_none_or(|focusable| focusable.enabled));
        let focused = focused.is_some_and(|focused| focused.0);
        let presented_interaction = if focused && *interaction == Interaction::None {
            Interaction::Hovered
        } else {
            *interaction
        };
        let requested = match (enabled, presented_interaction, alternate) {
            (false, _, false) => UiNodeVisualState::Disabled,
            (false, _, true) => UiNodeVisualState::AlternateDisabled,
            (true, Interaction::Pressed, false) => UiNodeVisualState::Activated,
            (true, Interaction::Pressed, true) => UiNodeVisualState::AlternateActivated,
            (true, Interaction::Hovered, false) => UiNodeVisualState::Highlighted,
            (true, Interaction::Hovered, true) => UiNodeVisualState::AlternateHighlighted,
            (true, _, false) => UiNodeVisualState::Normal,
            (true, _, true) => UiNodeVisualState::AlternateNormal,
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

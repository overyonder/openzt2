use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;

use super::authored_button_runtime_policy::UiButtonPolicy;
use super::authored_ui_focus_state::{UiFocusScope, UiFocusable};
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPreviousPointerInteraction(UiPointerInteraction);

impl UiPreviousPointerInteraction {
    pub fn from_current_interaction(interaction: UiPointerInteraction) -> Self {
        Self(interaction)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiPreviousPointerPressTimeSeconds(f64);

impl Default for UiPreviousPointerPressTimeSeconds {
    fn default() -> Self {
        Self(f64::NEG_INFINITY)
    }
}

pub(super) fn activate_authored_ui_nodes_from_pointer_interaction_changes(
    time: Res<Time<Real>>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    active_input: Res<openzt2_interface_state::input::input_types::ActiveInputDevice>,
    mut nodes: Query<
        (
            Entity,
            &UiPointerInteraction,
            Option<&UiInteractionEnabled>,
            Option<&UiFocusable>,
            Option<&UiButtonPolicy>,
            &UiDocumentOwner,
            &mut UiPreviousPointerInteraction,
            &mut UiPreviousPointerPressTimeSeconds,
        ),
        Changed<UiPointerInteraction>,
    >,
    mut scopes: Query<&mut UiFocusScope>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (
        entity,
        interaction,
        interaction_enabled,
        focusable,
        button,
        owner,
        mut previous,
        mut last_press,
    ) in &mut nodes
    {
        let enabled = context.visible_and_enabled(entity)
            && modal_input
                .0
                .is_none_or(|modal| context.is_in_modal(entity, modal))
            && interaction_enabled
                .map(|enabled| enabled.0)
                .unwrap_or_else(|| focusable.is_none_or(|focusable| focusable.enabled));
        let previous_interaction = previous.0;
        let effective_interaction = *interaction;
        let trigger = match (previous_interaction, effective_interaction) {
            (previous, UiPointerInteraction::Pressed)
                if enabled && previous != UiPointerInteraction::Pressed =>
            {
                Some(UiTrigger::Press)
            }
            (UiPointerInteraction::None, UiPointerInteraction::Hovered) if enabled => {
                Some(UiTrigger::Enter)
            }
            (previous, UiPointerInteraction::None) if previous != UiPointerInteraction::None => {
                Some(UiTrigger::Leave)
            }
            _ => None,
        };
        let momentary_trigger = button
            .filter(|policy| !policy.selectable)
            .and_then(|_| momentary_button_trigger(previous_interaction, effective_interaction))
            .filter(|_| enabled);
        previous.0 = effective_interaction;
        if let Some(trigger) = trigger {
            if trigger == UiTrigger::Press && focusable.is_some() {
                if let Ok(mut scope) = scopes.get_mut(context.document_scope(owner.0)) {
                    scope.focused = Some(entity);
                }
            }
            activated.write(UiNodeActivated {
                node: entity,
                source: active_input.source,
                trigger,
            });
            if trigger == UiTrigger::Press {
                let now = time.elapsed_secs_f64();
                if now - last_press.0 <= 0.5 {
                    activated.write(UiNodeActivated {
                        node: entity,
                        source: active_input.source,
                        trigger: UiTrigger::Submit,
                    });
                }
                last_press.0 = now;
            }
        }
        if let Some(trigger) = momentary_trigger {
            activated.write(UiNodeActivated {
                node: entity,
                source: active_input.source,
                trigger,
            });
        }
    }
}

const fn momentary_button_trigger(
    previous: UiPointerInteraction,
    current: UiPointerInteraction,
) -> Option<UiTrigger> {
    match (previous, current) {
        (UiPointerInteraction::Pressed, UiPointerInteraction::Pressed) => None,
        (_, UiPointerInteraction::Pressed) => Some(UiTrigger::On),
        (UiPointerInteraction::Pressed, _) => Some(UiTrigger::Off),
        _ => None,
    }
}

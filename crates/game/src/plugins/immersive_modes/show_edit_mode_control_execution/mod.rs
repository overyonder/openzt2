use bevy::prelude::*;

use crate::plugins::input::input_types::{ActionRequest, GameAction};

use super::{
    immersive_mode_control_types::ShowEditControl,
    immersive_mode_input_policy_operations::{
        interaction_action_flags_allow_requested_game_action, request_cancelled_immersive_mode_exit,
    },
    immersive_mode_message_types::ExitImmersiveMode,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::ActiveImmersiveMode,
};

pub(super) fn cancel_active_show_edit_mode(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    active_modes: Query<(Entity, Option<&AllowedInteractionActions>), With<ActiveImmersiveMode>>,
    show_edit_controls: Query<(), With<ShowEditControl>>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    let Ok((controller_entity, allowed_actions)) = active_modes.single() else {
        return;
    };
    if show_edit_controls.is_empty() {
        return;
    }
    for action_request in action_requests.read() {
        if !interaction_action_flags_allow_requested_game_action(
            allowed_actions,
            action_request.action,
        ) {
            continue;
        }
        if action_request.action == GameAction::Cancel {
            request_cancelled_immersive_mode_exit(controller_entity, &mut exit_requests);
        }
    }
}

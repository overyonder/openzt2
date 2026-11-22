use super::immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveSubject};
use bevy::prelude::*;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::{
    ActiveFossilSonarArtifactTracking, DiscoverFossilAtSiteRequest,
};

use super::immersive_mode_input_policy_operations::interaction_action_flags_allow_requested_game_action;
use super::immersive_mode_input_policy_operations::request_cancelled_immersive_mode_exit;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_policy_types::AllowedInteractionActions;
use super::immersive_mode_state_types::EquippedInteractionTool;
use crate::immersive_modes::immersive_control_execution_types::FossilAssemblyControl;
use crate::immersive_modes::immersive_control_execution_types::FossilSearchControl;

pub(super) fn discover_fossil_or_cancel_active_fossil_search_mode(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    active_modes: Query<
        (
            Entity,
            &ActiveFossilSonarArtifactTracking,
            Option<&AllowedInteractionActions>,
        ),
        (
            With<ActiveImmersiveMode>,
            With<EquippedInteractionTool>,
            With<FossilSearchControl>,
        ),
    >,
    mut fossil_discovery_requests: MessageWriter<DiscoverFossilAtSiteRequest>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    let Ok((controller_entity, tracking, allowed_actions)) = active_modes.single() else {
        return;
    };
    for action_request in action_requests.read() {
        if !interaction_action_flags_allow_requested_game_action(
            allowed_actions,
            action_request.action,
        ) {
            continue;
        }
        match action_request.action {
            GameAction::Confirm => {
                if let Some(fossil_site_entity) = tracking.selected_artifact {
                    fossil_discovery_requests.write(DiscoverFossilAtSiteRequest {
                        fossil_site: fossil_site_entity,
                        interaction_tool: controller_entity,
                    });
                }
            }
            GameAction::Cancel => {
                request_cancelled_immersive_mode_exit(controller_entity, &mut exit_requests);
            }
            _ => {}
        }
    }
}

pub(super) fn cancel_active_fossil_assembly_mode(
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    active_modes: Query<
        (Entity, Option<&AllowedInteractionActions>),
        (
            With<ActiveImmersiveMode>,
            With<EquippedInteractionTool>,
            With<FossilAssemblyControl>,
            With<ImmersiveSubject>,
        ),
    >,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    let Ok((controller_entity, allowed_actions)) = active_modes.single() else {
        return;
    };
    for action_request in action_requests.read() {
        if action_request.action == GameAction::Cancel
            && interaction_action_flags_allow_requested_game_action(
                allowed_actions,
                action_request.action,
            )
        {
            request_cancelled_immersive_mode_exit(controller_entity, &mut exit_requests);
        }
    }
}

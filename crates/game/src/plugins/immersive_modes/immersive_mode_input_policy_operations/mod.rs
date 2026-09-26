use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;

use crate::plugins::input::input_types::GameAction;

use super::{
    immersive_mode_message_types::{ExitImmersiveMode, ModeExitReason},
    immersive_mode_policy_types::AllowedInteractionActions,
};

#[inline]
pub(super) fn interaction_action_flags_allow_required_mode_action_flags(
    allowed_interaction_actions: Option<&AllowedInteractionActions>,
    required_mode_action_flags: ImmersiveModeActionFlags,
) -> bool {
    allowed_interaction_actions.is_none_or(|allowed_interaction_actions| {
        allowed_interaction_actions.allowed_actions.is_empty()
            || allowed_interaction_actions
                .allowed_actions
                .contains_any(required_mode_action_flags)
    })
}

#[inline]
pub(super) fn interaction_action_flags_allow_requested_game_action(
    allowed_interaction_actions: Option<&AllowedInteractionActions>,
    requested_game_action: GameAction,
) -> bool {
    let required_mode_action_flags = match requested_game_action {
        GameAction::NavigateUp => ImmersiveModeActionFlags::MOVE_FORWARD,
        GameAction::NavigateDown => ImmersiveModeActionFlags::MOVE_BACK,
        GameAction::NavigateLeft => ImmersiveModeActionFlags::STRAFE_LEFT,
        GameAction::NavigateRight => ImmersiveModeActionFlags::STRAFE_RIGHT,
        GameAction::Confirm => ImmersiveModeActionFlags::CONFIRM
            .with_additional_flags(ImmersiveModeActionFlags::PRIMARY),
        GameAction::Cancel | GameAction::OpenMenu => ImmersiveModeActionFlags::CANCEL,
        GameAction::RotateLeft
        | GameAction::RotateRight
        | GameAction::RotateObjectLeft
        | GameAction::RotateObjectRight => ImmersiveModeActionFlags::ROTATE,
        GameAction::ZoomIn | GameAction::ZoomOut => ImmersiveModeActionFlags::ZOOM,
        GameAction::Pause => return true,
        GameAction::UseObject | GameAction::PrimaryPointer => ImmersiveModeActionFlags::PRIMARY,
        GameAction::Undo
        | GameAction::Redo
        | GameAction::SecondaryPointer
        | GameAction::OverheadView
        | GameAction::OverviewMap
        | GameAction::DecreaseBrushSize
        | GameAction::IncreaseBrushSize => return false,
    };
    interaction_action_flags_allow_required_mode_action_flags(
        allowed_interaction_actions,
        required_mode_action_flags,
    )
}

pub(super) fn request_cancelled_immersive_mode_exit(
    controller_entity: Entity,
    exit_requests: &mut MessageWriter<ExitImmersiveMode>,
) {
    exit_requests.write(ExitImmersiveMode {
        controller: controller_entity,
        reason: ModeExitReason::Cancelled,
    });
}

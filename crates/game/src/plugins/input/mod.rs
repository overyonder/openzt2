//! Device-independent input translation over Bevy-owned device state.

mod controller_gameplay_actions;
mod controller_pointer;
mod gamepad_input_translation;
pub(crate) mod input_binding_reconfiguration_operations;
pub(crate) mod input_types;
mod keyboard_and_mouse_input_translation;

use bevy::prelude::*;

use crate::application_schedule::GameSet;
use crate::plugins::input::input_types::ActionRequest;

use input_types::{
    ActiveInputDevice, DeviceControlAxes, GameActionInputBindings,
    GameActionInputRebindingRejection, PrimaryPointerInputState, RebindGameActionInput,
};

pub struct GameInputPlugin;

impl Plugin for GameInputPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_resource::<GameActionInputBindings>()
            .init_resource::<DeviceControlAxes>()
            .init_resource::<PrimaryPointerInputState>()
            .init_resource::<ActiveInputDevice>()
            .init_resource::<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>()
            .add_message::<ActionRequest>()
            .add_message::<RebindGameActionInput>()
            .add_message::<GameActionInputRebindingRejection>()
            .add_systems(
                PreUpdate,
                (
                    crate::plugins::ui::active_authored_ui_context::capture_authored_modal_for_input_frame,
                    keyboard_and_mouse_input_translation::translate_keyboard_and_mouse_state_to_game_actions_and_control_axes,
                    gamepad_input_translation::translate_connected_gamepad_state_to_game_actions_and_control_axes,
                    controller_pointer::project_controller_pointer,
                    gamepad_input_translation::clear_control_axes_after_active_gamepad_disconnects,
                    controller_pointer::finalize_primary_pointer_edges,
                )
                    .chain()
                    .after(bevy::input::InputSystems)
                    .before(bevy::picking::PickingSystems::ProcessInput)
                    .in_set(GameSet::Input),
            )
            .add_systems(
                Update,
                controller_gameplay_actions::dispatch_controller_gameplay_actions.in_set(GameSet::Intent)
                    .run_if(in_state(crate::application_lifecycle::GamePhase::InGame)),
            )
            .add_systems(
                Update,
                input_binding_reconfiguration_operations::apply_requested_game_action_input_rebindings
                    .in_set(GameSet::Intent),
            );
    }
}

#[cfg(test)]
mod tests;

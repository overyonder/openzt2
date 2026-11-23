//! Device-independent input translation over Bevy-owned device state.

pub(crate) mod input_execution_types;

mod controller_gameplay_actions;
mod gamepad_input_translation;
pub mod input_binding_reconfiguration_operations;
mod shared_device_input;
use openzt2_interface_state::input::input_types;
pub(crate) mod keyboard_and_mouse_input_translation;

use bevy::{ecs::schedule::common_conditions::on_message, prelude::*};

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_world_state::application_schedule::GameSet;

use crate::input::input_execution_types::DeviceControlAxes;
use crate::input::input_execution_types::GameActionInputRebindingRejection;
use crate::input::input_execution_types::RebindGameActionInput;
use input_types::ActiveInputDevice;
use input_types::GameActionInputBindings;

pub struct GameInputPlugin;

impl Plugin for GameInputPlugin {
    fn build(&self, application: &mut App) {
        application
            .register_required_components::<Gamepad, gamepad_input_translation::GamepadControlSample>()
            .insert_resource(bevy::picking::input::PointerInputSettings { is_mouse_enabled: false, ..default() })
            .init_resource::<GameActionInputBindings>()
            .init_resource::<DeviceControlAxes>()
            .init_resource::<ActiveInputDevice>()
            .init_resource::<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>()
            .init_resource::<openzt2_interface_state::ui::input_frame_capture_types::OverheadModeInputCapture>()
            .add_message::<ActionRequest>()
            .add_message::<gamepad_input_translation::GamepadAction>()
            .add_message::<keyboard_and_mouse_input_translation::KeyboardMouseAction>()
            .add_message::<RebindGameActionInput>()
            .add_message::<GameActionInputRebindingRejection>()
            .add_systems(Startup, keyboard_and_mouse_input_translation::spawn_keyboard_mouse_device)
            .add_systems(
                PreUpdate,
                (
                    openzt2_ui::active_authored_ui_context::capture_authored_modal_for_input_frame
                        .before(gamepad_input_translation::translate_connected_gamepad_input),
                    openzt2_ui::active_authored_ui_context::capture_overhead_mode_for_input_frame,
                    (
                        keyboard_and_mouse_input_translation::translate_keyboard_mouse_input,
                        gamepad_input_translation::translate_connected_gamepad_input,
                    ).before(shared_device_input::publish_active_device_axes_and_pointer),
                    shared_device_input::publish_active_device_axes_and_pointer
                        .before(bevy::picking::PickingSystems::ProcessInput),
                    keyboard_and_mouse_input_translation::publish_keyboard_mouse_actions
                        .after(keyboard_and_mouse_input_translation::translate_keyboard_mouse_input)
                        .before(gamepad_input_translation::publish_gamepad_actions)
                        .run_if(on_message::<keyboard_and_mouse_input_translation::KeyboardMouseAction>),
                    gamepad_input_translation::publish_gamepad_actions
                        .after(gamepad_input_translation::translate_connected_gamepad_input)
                        .run_if(on_message::<gamepad_input_translation::GamepadAction>),
                )
                    .after(bevy::input::InputSystems)
                    .in_set(GameSet::Input),
            )
            .add_systems(
                Update,
                controller_gameplay_actions::dispatch_controller_gameplay_actions.in_set(GameSet::Intent)
                    .run_if(in_state(openzt2_game_assets::application_lifecycle::GamePhase::InGame)),
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

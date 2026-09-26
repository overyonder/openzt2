use bevy::{
    input::gamepad::{Gamepad, GamepadAxis, GamepadButton},
    prelude::*,
    time::Real,
};

use crate::plugins::input::input_types::{ActionRequest, ActionSource};

use super::input_types::{
    ActiveInputDevice, DeviceControlAxes, GameAction, GameActionInputBinding,
    GameActionInputBindings, InputChord,
};

const GAMEPAD_RADIAL_DEAD_ZONE: f32 = 0.20;

pub(super) fn translate_connected_gamepad_state_to_game_actions_and_control_axes(
    connected_gamepads: Query<(Entity, &Gamepad)>,
    action_bindings: Res<GameActionInputBindings>,
    real_time: Res<Time<Real>>,
    mut control_axes: ResMut<DeviceControlAxes>,
    mut active_input_device: ResMut<ActiveInputDevice>,
    mut action_requests: MessageWriter<ActionRequest>,
    phase: Res<State<crate::application_lifecycle::GamePhase>>,
    capture: Res<crate::plugins::ui::picking::UiPointerCapture>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
    mut navigation_repeat: Local<Option<(Entity, GameAction, f64)>>,
) {
    for (gamepad_entity, gamepad) in &connected_gamepads {
        let ui_navigation = *phase.get() != crate::application_lifecycle::GamePhase::InGame
            || capture.over_ui
            || modal_input.0.is_some();
        let right_stick = apply_radial_gamepad_dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::RightStickX).unwrap_or(0.0),
            gamepad.get(GamepadAxis::RightStickY).unwrap_or(0.0),
        ));
        let pan = if ui_navigation {
            Vec2::ZERO
        } else {
            right_stick
        };
        let pan_directions = [
            pan.y.max(0.0),
            (-pan.y).max(0.0),
            pan.x.max(0.0),
            (-pan.x).max(0.0),
        ];
        let look = Vec2::ZERO;
        let zoom = if ui_navigation {
            0.0
        } else {
            f32::from(game_action_is_held(
                GameAction::ZoomIn,
                &action_bindings,
                gamepad,
            )) - f32::from(game_action_is_held(
                GameAction::ZoomOut,
                &action_bindings,
                gamepad,
            ))
        };
        let menu_direction = if !ui_navigation {
            None
        } else if gamepad.pressed(GamepadButton::DPadUp) || right_stick.y > 0.5 {
            Some(GameAction::NavigateUp)
        } else if gamepad.pressed(GamepadButton::DPadDown) || right_stick.y < -0.5 {
            Some(GameAction::NavigateDown)
        } else if gamepad.pressed(GamepadButton::DPadLeft) || right_stick.x < -0.5 {
            Some(GameAction::NavigateLeft)
        } else if gamepad.pressed(GamepadButton::DPadRight) || right_stick.x > 0.5 {
            Some(GameAction::NavigateRight)
        } else {
            None
        };
        if let Some(direction) = menu_direction {
            let now = real_time.elapsed_secs_f64();
            let previous = *navigation_repeat;
            let repeated = previous
                .is_some_and(|(entity, action, _)| entity == gamepad_entity && action == direction);
            if !repeated || previous.is_some_and(|(_, _, next)| now >= next) {
                action_requests.write(ActionRequest {
                    action: direction,
                    source: ActionSource::Controller(gamepad_entity),
                });
                *navigation_repeat = Some((
                    gamepad_entity,
                    direction,
                    now + if repeated { 0.12 } else { 0.35 },
                ));
            }
        } else if navigation_repeat.is_some_and(|(entity, _, _)| entity == gamepad_entity) {
            *navigation_repeat = None;
        }
        if ui_navigation {
            for (button, action) in [
                (GamepadButton::South, GameAction::Confirm),
                (GamepadButton::East, GameAction::Cancel),
            ] {
                if gamepad.just_pressed(button) {
                    action_requests.write(ActionRequest {
                        action,
                        source: ActionSource::Controller(gamepad_entity),
                    });
                }
            }
        }
        let mut gamepad_input_was_meaningful = pan != Vec2::ZERO
            || look != Vec2::ZERO
            || zoom != 0.0
            || menu_direction.is_some()
            || gamepad.any_just_pressed([GamepadButton::South, GamepadButton::East]);
        for action_binding in action_bindings.entries.iter() {
            if modal_input.0.is_some()
                && !matches!(
                    action_binding.action,
                    GameAction::PrimaryPointer
                        | GameAction::Confirm
                        | GameAction::Cancel
                        | GameAction::NavigateUp
                        | GameAction::NavigateDown
                        | GameAction::NavigateLeft
                        | GameAction::NavigateRight
                )
            {
                continue;
            }
            if ui_navigation
                && matches!(
                    action_binding.action,
                    GameAction::UseObject
                        | GameAction::SecondaryPointer
                        | GameAction::Pause
                        | GameAction::Undo
                        | GameAction::ZoomIn
                        | GameAction::ZoomOut
                )
            {
                continue;
            }
            if game_action_binding_was_just_pressed_by_gamepad(action_binding, gamepad) {
                action_requests.write(ActionRequest {
                    action: action_binding.action,
                    source: ActionSource::Controller(gamepad_entity),
                });
                gamepad_input_was_meaningful = true;
            }
        }
        if gamepad_input_was_meaningful {
            *control_axes = DeviceControlAxes {
                pan,
                pan_directions,
                look,
                zoom,
            };
            active_input_device.source = ActionSource::Controller(gamepad_entity);
            active_input_device.changed_at = real_time.elapsed();
        }
    }
}

pub(super) fn clear_control_axes_after_active_gamepad_disconnects(
    connected_gamepads: Query<(), With<Gamepad>>,
    mut control_axes: ResMut<DeviceControlAxes>,
    mut active_input_device: ResMut<ActiveInputDevice>,
) {
    let ActionSource::Controller(active_gamepad_entity) = active_input_device.source else {
        return;
    };
    if !connected_gamepads.contains(active_gamepad_entity) {
        *control_axes = DeviceControlAxes::default();
        active_input_device.source = ActionSource::KeyboardMouse;
    }
}

pub(super) fn apply_radial_gamepad_dead_zone(raw_axis_value: Vec2) -> Vec2 {
    let raw_magnitude = raw_axis_value.length();
    if raw_magnitude <= GAMEPAD_RADIAL_DEAD_ZONE {
        return Vec2::ZERO;
    }
    let normalized_magnitude = ((raw_magnitude - GAMEPAD_RADIAL_DEAD_ZONE)
        / (1.0 - GAMEPAD_RADIAL_DEAD_ZONE))
        .clamp(0.0, 1.0);
    raw_axis_value / raw_magnitude * normalized_magnitude
}

fn game_action_binding_was_just_pressed_by_gamepad(
    action_binding: &GameActionInputBinding,
    gamepad: &Gamepad,
) -> bool {
    input_chord_was_just_pressed_by_gamepad(action_binding.primary, gamepad)
        || action_binding.alternate.is_some_and(|alternate_chord| {
            input_chord_was_just_pressed_by_gamepad(alternate_chord, gamepad)
        })
}

fn input_chord_was_just_pressed_by_gamepad(input_chord: InputChord, gamepad: &Gamepad) -> bool {
    match input_chord {
        InputChord::Gamepad(button) => gamepad.just_pressed(button),
        InputChord::Unbound
        | InputChord::Key(_)
        | InputChord::KeyPair(_, _)
        | InputChord::ModifiedKey { .. }
        | InputChord::Mouse(_) => false,
    }
}

pub(super) fn game_action_is_held(
    action: GameAction,
    bindings: &GameActionInputBindings,
    gamepad: &Gamepad,
) -> bool {
    bindings
        .entries
        .iter()
        .filter(|binding| binding.action == action)
        .any(|binding| {
            [Some(binding.primary), binding.alternate]
                .into_iter()
                .flatten()
                .any(
                    |chord| matches!(chord, InputChord::Gamepad(button) if gamepad.pressed(button)),
                )
        })
}

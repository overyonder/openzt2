use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
    time::Real,
    window::PrimaryWindow,
};

use crate::plugins::input::input_types::{ActionRequest, ActionSource, GameAction};

use super::input_types::{
    ActiveInputDevice, DeviceControlAxes, GameActionInputBinding, GameActionInputBindings,
    InputChord, PrimaryPointerInputState,
};

const MOUSE_LOOK_PIXELS_FOR_FULL_SCALE: f32 = 32.0;

// Device input, focus and bindings are sampled in the same input pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn translate_keyboard_and_mouse_state_to_game_actions_and_control_axes(
    keyboard_keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    focus: Res<InputFocus>,
    text_edits: Query<(), With<EditableText>>,
    accumulated_mouse_motion: Res<AccumulatedMouseMotion>,
    accumulated_mouse_scroll: Res<AccumulatedMouseScroll>,
    action_bindings: Res<GameActionInputBindings>,
    real_time: Res<Time<Real>>,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    mut control_axes: ResMut<DeviceControlAxes>,
    mut pointer_input: ResMut<PrimaryPointerInputState>,
    mut active_input_device: ResMut<ActiveInputDevice>,
    mut action_requests: MessageWriter<ActionRequest>,
) {
    pointer_input.available =
        primary_window
            .physical_cursor_position()
            .is_some_and(|physical_cursor_position| {
                pointer_input.screen = physical_cursor_position;
                true
            });
    pointer_input.pressed = mouse_buttons.pressed(MouseButton::Left);
    pointer_input.wheel_y = accumulated_mouse_scroll.delta.y;

    let editing_text = focus
        .get()
        .is_some_and(|focused| text_edits.contains(focused));
    let empty_keyboard = ButtonInput::<KeyCode>::default();
    let gameplay_keyboard = if editing_text {
        &empty_keyboard
    } else {
        &*keyboard_keys
    };

    let pan_directions = [
        read_digital_game_action_magnitude(
            &action_bindings,
            GameAction::NavigateUp,
            gameplay_keyboard,
            &mouse_buttons,
        ),
        read_digital_game_action_magnitude(
            &action_bindings,
            GameAction::NavigateDown,
            gameplay_keyboard,
            &mouse_buttons,
        ),
        read_digital_game_action_magnitude(
            &action_bindings,
            GameAction::NavigateRight,
            gameplay_keyboard,
            &mouse_buttons,
        ),
        read_digital_game_action_magnitude(
            &action_bindings,
            GameAction::NavigateLeft,
            gameplay_keyboard,
            &mouse_buttons,
        ),
    ];
    let pan = Vec2::new(
        pan_directions[2] - pan_directions[3],
        pan_directions[0] - pan_directions[1],
    );
    let look = if mouse_buttons.pressed(MouseButton::Right) {
        (accumulated_mouse_motion.delta / MOUSE_LOOK_PIXELS_FOR_FULL_SCALE)
            .clamp(Vec2::splat(-1.0), Vec2::ONE)
    } else {
        Vec2::ZERO
    };
    let zoom = (read_digital_game_action_magnitude(
        &action_bindings,
        GameAction::ZoomIn,
        gameplay_keyboard,
        &mouse_buttons,
    ) - read_digital_game_action_magnitude(
        &action_bindings,
        GameAction::ZoomOut,
        gameplay_keyboard,
        &mouse_buttons,
    ))
    .clamp(-1.0, 1.0);
    *control_axes = DeviceControlAxes {
        pan,
        pan_directions,
        look,
        zoom,
    };

    let mut keyboard_or_mouse_input_was_meaningful =
        pan_directions.iter().any(|magnitude| *magnitude > 0.0)
            || look != Vec2::ZERO
            || zoom != 0.0
            || pointer_input.wheel_y != 0.0
            || accumulated_mouse_motion.delta != Vec2::ZERO
            || mouse_buttons.just_pressed(MouseButton::Left);

    if editing_text {
        for (pressed, action) in [
            (
                keyboard_keys.just_pressed(KeyCode::Enter)
                    || keyboard_keys.just_pressed(KeyCode::NumpadEnter),
                GameAction::Confirm,
            ),
            (
                keyboard_keys.just_pressed(KeyCode::Escape),
                GameAction::Cancel,
            ),
        ] {
            if pressed {
                action_requests.write(ActionRequest {
                    action,
                    source: ActionSource::KeyboardMouse,
                });
                keyboard_or_mouse_input_was_meaningful = true;
            }
        }
    }
    for action_binding in action_bindings.entries.iter() {
        if game_action_binding_was_just_pressed_by_keyboard_or_mouse(
            action_binding,
            gameplay_keyboard,
            &mouse_buttons,
        ) {
            action_requests.write(ActionRequest {
                action: action_binding.action,
                source: ActionSource::KeyboardMouse,
            });
            keyboard_or_mouse_input_was_meaningful = true;
        }
    }
    if keyboard_or_mouse_input_was_meaningful {
        active_input_device.source = ActionSource::KeyboardMouse;
        active_input_device.changed_at = real_time.elapsed();
    }
}

pub(super) fn read_digital_game_action_magnitude(
    action_bindings: &GameActionInputBindings,
    requested_action: GameAction,
    keyboard_keys: &ButtonInput<KeyCode>,
    mouse_buttons: &ButtonInput<MouseButton>,
) -> f32 {
    action_bindings
        .entries
        .iter()
        .find(|binding| binding.action == requested_action)
        .is_some_and(|binding| {
            input_chord_is_pressed_by_keyboard_or_mouse(
                binding.primary,
                keyboard_keys,
                mouse_buttons,
            ) || binding.alternate.is_some_and(|alternate_chord| {
                input_chord_is_pressed_by_keyboard_or_mouse(
                    alternate_chord,
                    keyboard_keys,
                    mouse_buttons,
                )
            })
        })
        .into()
}

pub(super) fn game_action_binding_was_just_pressed_by_keyboard_or_mouse(
    action_binding: &GameActionInputBinding,
    keyboard_keys: &ButtonInput<KeyCode>,
    mouse_buttons: &ButtonInput<MouseButton>,
) -> bool {
    input_chord_was_just_pressed_by_keyboard_or_mouse(
        action_binding.primary,
        keyboard_keys,
        mouse_buttons,
    ) || action_binding.alternate.is_some_and(|alternate_chord| {
        input_chord_was_just_pressed_by_keyboard_or_mouse(
            alternate_chord,
            keyboard_keys,
            mouse_buttons,
        )
    })
}

fn input_chord_is_pressed_by_keyboard_or_mouse(
    input_chord: InputChord,
    keyboard_keys: &ButtonInput<KeyCode>,
    mouse_buttons: &ButtonInput<MouseButton>,
) -> bool {
    match input_chord {
        InputChord::Key(key) => keyboard_keys.pressed(key),
        InputChord::KeyPair(first_key, second_key) => {
            keyboard_keys.pressed(first_key) || keyboard_keys.pressed(second_key)
        }
        InputChord::ModifiedKey {
            key,
            shift,
            control,
            alt,
        } => {
            keyboard_keys.pressed(key)
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::ShiftLeft,
                    KeyCode::ShiftRight,
                ) == shift
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::ControlLeft,
                    KeyCode::ControlRight,
                ) == control
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::AltLeft,
                    KeyCode::AltRight,
                ) == alt
        }
        InputChord::Mouse(button) => mouse_buttons.pressed(button),
        InputChord::Gamepad(_) | InputChord::Unbound => false,
    }
}

fn input_chord_was_just_pressed_by_keyboard_or_mouse(
    input_chord: InputChord,
    keyboard_keys: &ButtonInput<KeyCode>,
    mouse_buttons: &ButtonInput<MouseButton>,
) -> bool {
    match input_chord {
        InputChord::Key(key) => keyboard_keys.just_pressed(key),
        InputChord::KeyPair(first_key, second_key) => {
            keyboard_keys.just_pressed(first_key) || keyboard_keys.just_pressed(second_key)
        }
        InputChord::ModifiedKey {
            key,
            shift,
            control,
            alt,
        } => {
            keyboard_keys.just_pressed(key)
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::ShiftLeft,
                    KeyCode::ShiftRight,
                ) == shift
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::ControlLeft,
                    KeyCode::ControlRight,
                ) == control
                && either_keyboard_modifier_key_is_pressed(
                    keyboard_keys,
                    KeyCode::AltLeft,
                    KeyCode::AltRight,
                ) == alt
        }
        InputChord::Mouse(button) => mouse_buttons.just_pressed(button),
        InputChord::Gamepad(_) | InputChord::Unbound => false,
    }
}

fn either_keyboard_modifier_key_is_pressed(
    keyboard_keys: &ButtonInput<KeyCode>,
    left_modifier_key: KeyCode,
    right_modifier_key: KeyCode,
) -> bool {
    keyboard_keys.pressed(left_modifier_key) || keyboard_keys.pressed(right_modifier_key)
}

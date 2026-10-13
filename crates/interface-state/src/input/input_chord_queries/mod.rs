use super::input_types::InputChord;
use bevy::prelude::*;

pub fn input_chord_was_just_pressed_by_keyboard_or_mouse(
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

pub fn either_keyboard_modifier_key_is_pressed(
    keyboard_keys: &ButtonInput<KeyCode>,
    left_modifier_key: KeyCode,
    right_modifier_key: KeyCode,
) -> bool {
    keyboard_keys.pressed(left_modifier_key) || keyboard_keys.pressed(right_modifier_key)
}

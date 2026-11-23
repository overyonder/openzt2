use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
};
use openzt2_interface_state::input::input_chord_queries::either_keyboard_modifier_key_is_pressed;
use openzt2_interface_state::input::input_chord_queries::input_chord_was_just_pressed_by_keyboard_or_mouse;

use openzt2_interface_state::camera::camera_runtime_state_types::{CameraMode, ZooCamera};
use openzt2_interface_state::input::input_types::{ActionRequest, ActionSource, GameAction};

use super::input_types::GameActionInputBinding;
use super::input_types::GameActionInputBindings;
use super::input_types::InputChord;
use crate::input::input_execution_types::DeviceControlAxes;

const MOUSE_LOOK_PIXELS_FOR_FULL_SCALE: f32 = 32.0;

#[derive(Component, Default, PartialEq)]
pub(super) struct KeyboardMouseControlSample {
    pub(super) axes: DeviceControlAxes,
    pub(super) meaningful: bool,
}

#[derive(Message)]
pub(super) struct KeyboardMouseAction(GameAction);

pub(super) fn spawn_keyboard_mouse_device(mut commands: Commands) {
    commands.spawn(KeyboardMouseControlSample::default());
}

pub(super) fn translate_keyboard_mouse_input(
    keyboard_keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    focus: Res<InputFocus>,
    text_edits: Query<(), With<EditableText>>,
    accumulated_mouse_motion: Res<AccumulatedMouseMotion>,
    accumulated_mouse_scroll: Res<AccumulatedMouseScroll>,
    bindings: Res<GameActionInputBindings>,
    cameras: Query<&CameraMode, With<ZooCamera>>,
    mut samples: Query<&mut KeyboardMouseControlSample>,
    mut actions: MessageWriter<KeyboardMouseAction>,
) {
    let Ok(mut sample) = samples.single_mut() else {
        return;
    };
    let editing_text = focus.get().is_some_and(|node| text_edits.contains(node));
    let empty_keyboard = ButtonInput::<KeyCode>::default();
    let keyboard = if editing_text {
        &empty_keyboard
    } else {
        &keyboard_keys
    };
    let mut sent_action = false;
    let mut publish = |action| {
        actions.write(KeyboardMouseAction(action));
        sent_action = true;
    };
    if editing_text {
        if keyboard_keys.just_pressed(KeyCode::Enter)
            || keyboard_keys.just_pressed(KeyCode::NumpadEnter)
        {
            publish(GameAction::Confirm);
        }
        if keyboard_keys.just_pressed(KeyCode::Escape) {
            publish(GameAction::Cancel);
        }
    }
    let first_person = cameras
        .single()
        .is_ok_and(|mode| *mode == CameraMode::FirstPerson);
    let mut magnitude = [None; 6];
    for binding in bindings.entries.iter() {
        let axis = match binding.action {
            GameAction::NavigateUp => Some(0),
            GameAction::NavigateDown => Some(1),
            GameAction::NavigateRight => Some(2),
            GameAction::NavigateLeft => Some(3),
            GameAction::ZoomIn => Some(4),
            GameAction::ZoomOut => Some(5),
            _ => None,
        };
        if let Some(axis) = axis.filter(|axis| magnitude[*axis].is_none()) {
            let pressed = input_chord_is_pressed_by_keyboard_or_mouse(
                binding.primary,
                keyboard,
                &mouse_buttons,
            ) || binding.alternate.is_some_and(|chord| {
                input_chord_is_pressed_by_keyboard_or_mouse(chord, keyboard, &mouse_buttons)
            });
            magnitude[axis] = Some(f32::from(pressed));
        }
        if (first_person || binding.action != GameAction::UseObject)
            && game_action_binding_was_just_pressed_by_keyboard_or_mouse(
                binding,
                keyboard,
                &mouse_buttons,
            )
        {
            publish(binding.action);
        }
    }
    let [forward, back, right, left, zoom_in, zoom_out] = magnitude.map(Option::unwrap_or_default);
    let pan_directions = [forward, back, right, left];
    let look = if mouse_buttons.pressed(MouseButton::Right) {
        (accumulated_mouse_motion.delta / MOUSE_LOOK_PIXELS_FOR_FULL_SCALE)
            .clamp(Vec2::splat(-1.0), Vec2::ONE)
    } else {
        Vec2::ZERO
    };
    let zoom = (zoom_in - zoom_out).clamp(-1.0, 1.0);
    sample.set_if_neq(KeyboardMouseControlSample {
        axes: DeviceControlAxes {
            pan: Vec2::new(right - left, forward - back),
            pan_directions,
            look,
            zoom,
        },
        meaningful: pan_directions.iter().any(|value| *value > 0.0)
            || accumulated_mouse_motion.delta != Vec2::ZERO
            || accumulated_mouse_scroll.delta.y != 0.0
            || mouse_buttons.just_pressed(MouseButton::Left)
            || zoom != 0.0
            || sent_action,
    });
}

pub(super) fn publish_keyboard_mouse_actions(
    mut input: MessageReader<KeyboardMouseAction>,
    mut actions: MessageWriter<ActionRequest>,
) {
    let _ = actions.write_batch(input.read().map(|action| ActionRequest {
        action: action.0,
        source: ActionSource::KeyboardMouse,
    }));
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

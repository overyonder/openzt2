use bevy::{
    camera::NormalizedRenderTarget,
    ecs::system::SystemParam,
    input::{
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
        ButtonState,
    },
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    prelude::*,
    window::PrimaryWindow,
};

use super::verification_capture_target::VerificationPointerMarker;

/// Writes scripted input through the same Bevy messages the window backend produces.
#[derive(SystemParam)]
pub(crate) struct VerificationInputInjection<'w, 's> {
    primary_window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    ui_scale: Res<'w, UiScale>,
    pointer_inputs: MessageWriter<'w, PointerInput>,
    mouse_buttons: MessageWriter<'w, MouseButtonInput>,
    mouse_motion: MessageWriter<'w, MouseMotion>,
    keyboard_inputs: MessageWriter<'w, KeyboardInput>,
    mouse_wheels: MessageWriter<'w, MouseWheel>,
    markers: Query<'w, 's, &'static mut Node, With<VerificationPointerMarker>>,
}

impl VerificationInputInjection<'_, '_> {
    pub(crate) fn move_pointer(&mut self, target: &NormalizedRenderTarget, from: Vec2, to: Vec2) {
        let marker_position = to / self.ui_scale.0;
        for mut node in &mut self.markers {
            node.left = px(marker_position.x - 7.0);
            node.top = px(marker_position.y - 7.0);
        }
        self.primary_window.1.set_cursor_position(Some(to));
        self.mouse_motion.write(MouseMotion { delta: to - from });
        self.pointer_inputs.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position: to,
            },
            PointerAction::Move { delta: to - from },
        ));
    }

    pub(crate) fn press_or_release_primary_button(
        &mut self,
        target: &NormalizedRenderTarget,
        position: Vec2,
        pressed: bool,
    ) {
        self.pointer_inputs.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position,
            },
            if pressed {
                PointerAction::Press(PointerButton::Primary)
            } else {
                PointerAction::Release(PointerButton::Primary)
            },
        ));
        self.mouse_buttons.write(MouseButtonInput {
            button: MouseButton::Left,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            window: self.primary_window.0,
        });
    }

    pub(crate) fn press_or_release_key(&mut self, key_code: KeyCode, pressed: bool) {
        self.keyboard_inputs.write(KeyboardInput {
            key_code,
            logical_key: logical_key_for_verification_key_code(key_code),
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat: false,
            window: self.primary_window.0,
        });
    }

    pub(crate) fn press_or_release_text_character(&mut self, character: char, pressed: bool) {
        let text: smol_str::SmolStr = character.to_string().into();
        self.keyboard_inputs.write(KeyboardInput {
            key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
            logical_key: if character == ' ' {
                Key::Space
            } else {
                Key::Character(text.clone())
            },
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: pressed.then_some(text),
            repeat: false,
            window: self.primary_window.0,
        });
    }

    pub(crate) fn scroll_wheel_lines(&mut self, lines: f32) {
        self.mouse_wheels.write(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: lines,
            window: self.primary_window.0,
            phase: TouchPhase::Moved,
        });
    }

    /// An offscreen target has no window backend to refresh a stationary cursor,
    /// so picking re-evaluates the same location every frame, as it would natively.
    pub(crate) fn resubmit_stationary_pointer(
        &mut self,
        target: &NormalizedRenderTarget,
        position: Vec2,
    ) {
        self.pointer_inputs.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position,
            },
            PointerAction::Move { delta: Vec2::ZERO },
        ));
    }

    pub(crate) fn primary_window_entity(&self) -> Entity {
        self.primary_window.0
    }
}

fn logical_key_for_verification_key_code(key_code: KeyCode) -> Key {
    match key_code {
        KeyCode::Escape => Key::Escape,
        KeyCode::Space => Key::Space,
        KeyCode::Enter => Key::Enter,
        KeyCode::Delete => Key::Delete,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Equal => Key::Character("=".into()),
        KeyCode::Minus => Key::Character("-".into()),
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::KeyA => Key::Character("a".into()),
        KeyCode::KeyD => Key::Character("d".into()),
        KeyCode::KeyP => Key::Character("p".into()),
        KeyCode::KeyS => Key::Character("s".into()),
        KeyCode::KeyW => Key::Character("w".into()),
        KeyCode::KeyY => Key::Character("y".into()),
        KeyCode::KeyZ => Key::Character("z".into()),
        _ => Key::Unidentified(NativeKey::Unidentified),
    }
}

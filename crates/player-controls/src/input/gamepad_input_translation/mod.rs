use std::time::Duration;

use crate::input::input_execution_types::DeviceControlAxes;
use bevy::{
    input::gamepad::{Gamepad, GamepadAxis, GamepadButton},
    prelude::*,
    time::Real,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::{
    input::input_types::{GameAction, GameActionInputBindings, InputChord},
    ui::{
        input_frame_capture_types::AuthoredModalInputCapture,
        pointer_capture_types::UiPointerCapture,
    },
};

const GAMEPAD_RADIAL_DEAD_ZONE: f32 = 0.20;

#[derive(Component, Default)]
#[require(GamepadNavigationRepeat)]
pub(super) struct GamepadInputSample {
    pub(super) pointer_motion: Vec2,
    pub(super) primary: bool,
    pub(super) secondary: bool,
    pub(super) axes: DeviceControlAxes,
    pub(super) meaningful: bool,
    pub(super) actions: Vec<GameAction>,
}

#[derive(Component, Default)]
pub(super) struct GamepadNavigationRepeat(Option<(GameAction, Timer)>);

pub(super) fn sample_connected_gamepads(
    mut devices: Query<(
        Ref<Gamepad>,
        &mut GamepadInputSample,
        &mut GamepadNavigationRepeat,
    )>,
    bindings: Res<GameActionInputBindings>,
    time: Res<Time<Real>>,
    phase: Res<State<GamePhase>>,
    capture: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
) {
    let ui = *phase.get() != GamePhase::InGame || capture.over_ui || modal.0.is_some();
    for (gamepad, mut sample, mut repeat) in &mut devices {
        if gamepad.is_added() {
            repeat.0 = None;
        }
        sample.actions.clear();
        sample.pointer_motion = radial_dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::LeftStickX).unwrap_or_default(),
            -gamepad.get(GamepadAxis::LeftStickY).unwrap_or_default(),
        ));
        let right = radial_dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::RightStickX).unwrap_or_default(),
            gamepad.get(GamepadAxis::RightStickY).unwrap_or_default(),
        ));
        let pan = if ui { Vec2::ZERO } else { right };
        let navigation = if !ui {
            None
        } else if gamepad.pressed(GamepadButton::DPadUp) || right.y > 0.5 {
            Some(GameAction::NavigateUp)
        } else if gamepad.pressed(GamepadButton::DPadDown) || right.y < -0.5 {
            Some(GameAction::NavigateDown)
        } else if gamepad.pressed(GamepadButton::DPadLeft) || right.x < -0.5 {
            Some(GameAction::NavigateLeft)
        } else if gamepad.pressed(GamepadButton::DPadRight) || right.x > 0.5 {
            Some(GameAction::NavigateRight)
        } else {
            None
        };
        if let Some(direction) = navigation {
            let emit = match &mut repeat.0 {
                Some((previous, timer)) if *previous == direction => {
                    if timer.tick(time.delta()).is_finished() {
                        timer.set_duration(Duration::from_millis(120));
                        timer.reset();
                        true
                    } else {
                        false
                    }
                }
                _ => {
                    repeat.0 = Some((
                        direction,
                        Timer::new(Duration::from_millis(350), TimerMode::Once),
                    ));
                    true
                }
            };
            if emit {
                sample.actions.push(direction);
            }
        } else if repeat.0.is_some() {
            repeat.0 = None;
        }
        if ui {
            for (button, action) in [
                (GamepadButton::South, GameAction::Confirm),
                (GamepadButton::East, GameAction::Cancel),
            ] {
                if gamepad.just_pressed(button) {
                    sample.actions.push(action);
                }
            }
        }
        let mut zoom_in = false;
        let mut zoom_out = false;
        sample.primary = false;
        sample.secondary = false;
        for binding in bindings.entries.iter() {
            let buttons = [Some(binding.primary), binding.alternate]
                .into_iter()
                .flatten()
                .filter_map(|chord| match chord {
                    InputChord::Gamepad(button) => Some(button),
                    _ => None,
                });
            for button in buttons.clone() {
                if !gamepad.pressed(button) {
                    continue;
                }
                match binding.action {
                    GameAction::ZoomIn => zoom_in = true,
                    GameAction::ZoomOut => zoom_out = true,
                    GameAction::PrimaryPointer => sample.primary = true,
                    GameAction::SecondaryPointer => sample.secondary = !ui,
                    _ => {}
                }
            }
            if modal.0.is_some()
                && !matches!(
                    binding.action,
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
            if ui
                && matches!(
                    binding.action,
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
            if buttons
                .into_iter()
                .any(|button| gamepad.just_pressed(button))
            {
                sample.actions.push(binding.action);
            }
        }
        let zoom = if ui {
            0.0
        } else {
            f32::from(zoom_in) - f32::from(zoom_out)
        };
        sample.axes = DeviceControlAxes {
            pan,
            pan_directions: [
                pan.y.max(0.0),
                (-pan.y).max(0.0),
                pan.x.max(0.0),
                (-pan.x).max(0.0),
            ],
            look: Vec2::ZERO,
            zoom,
        };
        sample.meaningful = pan != Vec2::ZERO
            || zoom != 0.0
            || navigation.is_some()
            || gamepad.any_just_pressed([GamepadButton::South, GamepadButton::East])
            || !sample.actions.is_empty();
    }
}

pub(super) fn radial_dead_zone(raw: Vec2) -> Vec2 {
    let magnitude = raw.length();
    if magnitude <= GAMEPAD_RADIAL_DEAD_ZONE {
        return Vec2::ZERO;
    }
    let scaled =
        ((magnitude - GAMEPAD_RADIAL_DEAD_ZONE) / (1.0 - GAMEPAD_RADIAL_DEAD_ZONE)).clamp(0.0, 1.0);
    raw / magnitude * scaled
}

use std::time::Duration;

use crate::input::input_execution_types::DeviceControlAxes;
use bevy::{
    input::gamepad::{Gamepad, GamepadAxis, GamepadButton},
    prelude::*,
    time::Real,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::{
    input::input_types::{
        ActionRequest, ActionSource, GameAction, GameActionInputBindings, InputChord,
    },
    ui::{
        input_frame_capture_types::AuthoredModalInputCapture,
        pointer_capture_types::UiPointerCapture,
    },
};

const GAMEPAD_RADIAL_DEAD_ZONE: f32 = 0.20;

#[derive(Component, Default, PartialEq)]
#[require(GamepadNavigationRepeat)]
pub(super) struct GamepadControlSample {
    pub(super) pointer_motion: Vec2,
    pub(super) primary: bool,
    pub(super) secondary: bool,
    pub(super) axes: DeviceControlAxes,
    pub(super) meaningful: bool,
}

#[derive(Component, Default)]
pub(super) struct GamepadNavigationRepeat(Option<(GameAction, Timer)>);

#[derive(Message)]
pub(super) struct GamepadAction {
    device: Entity,
    action: GameAction,
}

pub(super) fn translate_connected_gamepad_input(
    mut devices: Query<(
        Entity,
        Ref<Gamepad>,
        &mut GamepadControlSample,
        &mut GamepadNavigationRepeat,
    )>,
    bindings: Res<GameActionInputBindings>,
    time: Res<Time<Real>>,
    phase: Res<State<GamePhase>>,
    capture: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
    mut actions: MessageWriter<GamepadAction>,
) {
    let ui = *phase.get() != GamePhase::InGame || capture.over_ui || modal.0.is_some();
    for (device, gamepad, mut sample, mut repeat) in &mut devices {
        if gamepad.is_added() {
            repeat.0 = None;
        }
        let pointer_motion = radial_dead_zone(Vec2::new(
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
        let mut sent_action = false;
        let mut publish = |action| {
            actions.write(GamepadAction { device, action });
            sent_action = true;
        };
        if let Some(direction) = navigation {
            match &mut repeat.0 {
                Some((previous, timer)) if *previous == direction => {
                    if timer.tick(time.delta()).is_finished() {
                        timer.set_duration(Duration::from_millis(120));
                        timer.reset();
                        publish(direction);
                    }
                }
                _ => {
                    repeat.0 = Some((
                        direction,
                        Timer::new(Duration::from_millis(350), TimerMode::Once),
                    ));
                    publish(direction);
                }
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
                    publish(action);
                }
            }
        }
        let mut zoom_in = false;
        let mut zoom_out = false;
        let mut primary = false;
        let mut secondary = false;
        for binding in bindings.entries.iter() {
            let buttons = [Some(binding.primary), binding.alternate]
                .into_iter()
                .flatten()
                .filter_map(|chord| match chord {
                    InputChord::Gamepad(button) => Some(button),
                    _ => None,
                });
            if buttons.clone().any(|button| gamepad.pressed(button)) {
                match binding.action {
                    GameAction::ZoomIn => zoom_in = true,
                    GameAction::ZoomOut => zoom_out = true,
                    GameAction::PrimaryPointer => primary = true,
                    GameAction::SecondaryPointer => secondary = !ui,
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
                publish(binding.action);
            }
        }
        let zoom = if ui {
            0.0
        } else {
            f32::from(zoom_in) - f32::from(zoom_out)
        };
        sample.set_if_neq(GamepadControlSample {
            pointer_motion,
            primary,
            secondary,
            axes: DeviceControlAxes {
                pan,
                pan_directions: [
                    pan.y.max(0.0),
                    (-pan.y).max(0.0),
                    pan.x.max(0.0),
                    (-pan.x).max(0.0),
                ],
                look: Vec2::ZERO,
                zoom,
            },
            meaningful: pan != Vec2::ZERO
                || zoom != 0.0
                || navigation.is_some()
                || gamepad.any_just_pressed([GamepadButton::South, GamepadButton::East])
                || sent_action,
        });
    }
}

pub(super) fn publish_gamepad_actions(
    mut input: MessageReader<GamepadAction>,
    mut actions: MessageWriter<ActionRequest>,
) {
    let _ = actions.write_batch(input.read().map(|input| ActionRequest {
        action: input.action,
        source: ActionSource::Controller(input.device),
    }));
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

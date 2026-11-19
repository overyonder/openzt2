use super::gamepad_input_translation::GamepadInputSample;
use crate::input::input_execution_types::DeviceControlAxes;
use bevy::{
    camera::RenderTarget,
    input::mouse::MouseWheel,
    picking::pointer::{
        Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation,
        PointerPressState,
    },
    prelude::*,
    window::{PrimaryWindow, WindowRef},
};
use openzt2_interface_state::input::input_types::{ActionSource, ActiveInputDevice};

pub(super) fn emit_shared_pointer_input(
    time: Res<Time<Real>>,
    gamepads: Query<&GamepadInputSample, With<Gamepad>>,
    active: Res<ActiveInputDevice>,
    physical_buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut pointer: Query<(&PointerId, &mut PointerLocation, &PointerPressState)>,
    mut axes: ResMut<DeviceControlAxes>,
    mut events: MessageWriter<PointerInput>,
) {
    let Ok((window_entity, mut window)) = windows.single_mut() else {
        return;
    };
    let Some((_, mut previous_location, held)) = pointer.iter_mut().find(|(id, ..)| id.is_mouse())
    else {
        return;
    };
    let Some(target) =
        RenderTarget::Window(WindowRef::Entity(window_entity)).normalize(Some(window_entity))
    else {
        return;
    };
    let controller = match active.source {
        ActionSource::Controller(entity) => gamepads.get(entity).ok(),
        _ => None,
    };
    let scale = window.scale_factor();
    let previous = previous_location
        .location()
        .map(|location| location.position);
    let physical = window.cursor_position();
    let position = if let Some(controller) = controller {
        let current = physical
            .or(previous)
            .unwrap_or(Vec2::new(window.width(), window.height()) * 0.5);
        let next = (current + controller.pointer_motion * (600.0 * time.delta_secs())).clamp(
            Vec2::ZERO,
            Vec2::new(
                window.physical_width().saturating_sub(1) as f32,
                window.physical_height().saturating_sub(1) as f32,
            ) / scale,
        );
        if controller.secondary {
            axes.look = ((next - current) / 32.0).clamp(Vec2::splat(-1.0), Vec2::ONE);
        }
        if physical != Some(next) {
            window.set_cursor_position(Some(next));
        }
        Some(next)
    } else {
        physical
    };
    let next = [
        physical_buttons.pressed(MouseButton::Left)
            || controller.is_some_and(|controller| controller.primary),
        physical_buttons.pressed(MouseButton::Right)
            || controller.is_some_and(|controller| controller.secondary),
        physical_buttons.pressed(MouseButton::Middle),
    ];
    let was_pressed = [
        held.is_primary_pressed(),
        held.is_secondary_pressed(),
        held.is_middle_pressed(),
    ];
    let location = Location {
        target,
        position: position.or(previous).unwrap_or_default(),
    };
    if let Some(position) = position {
        if previous_location.location() != Some(&location) {
            events.write(PointerInput::new(
                PointerId::Mouse,
                location.clone(),
                PointerAction::Move {
                    delta: previous.map_or(Vec2::ZERO, |previous| position - previous),
                },
            ));
        }
    } else if previous.is_some() {
        previous_location.location = None;
    }
    for (index, button) in PointerButton::iter().enumerate() {
        if next[index] != was_pressed[index] {
            events.write(PointerInput::new(
                PointerId::Mouse,
                location.clone(),
                if next[index] {
                    PointerAction::Press(button)
                } else {
                    PointerAction::Release(button)
                },
            ));
        }
    }
    for event in wheel.read().filter(|event| event.window == window_entity) {
        events.write(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            PointerAction::Scroll {
                x: event.x,
                y: event.y,
                unit: event.unit,
                phase: event.phase,
            },
        ));
    }
}

pub(super) fn arbitrate_sampled_input(
    keyboard: Res<super::keyboard_and_mouse_input_translation::KeyboardMouseSample>,
    pads: Query<(Entity, &GamepadInputSample), With<Gamepad>>,
    time: Res<Time<Real>>,
    mut source: ResMut<ActiveInputDevice>,
    mut axes: ResMut<DeviceControlAxes>,
    mut actions: MessageWriter<openzt2_interface_state::input::input_types::ActionRequest>,
) {
    use openzt2_interface_state::input::input_types::ActionRequest;

    axes.set_if_neq(keyboard.axes);
    if keyboard.meaningful {
        source.set_if_neq(ActiveInputDevice {
            source: ActionSource::KeyboardMouse,
            changed_at: time.elapsed(),
        });
    }
    let _ = actions.write_batch(
        keyboard
            .actions
            .iter()
            .copied()
            .map(|action| ActionRequest {
                action,
                source: ActionSource::KeyboardMouse,
            }),
    );
    let mut pointer_owner = None;
    for (entity, pad) in &pads {
        let controller = ActionSource::Controller(entity);
        if pad.meaningful {
            axes.set_if_neq(pad.axes);
            source.set_if_neq(ActiveInputDevice {
                source: controller,
                changed_at: time.elapsed(),
            });
        }
        let _ = actions.write_batch(pad.actions.iter().copied().map(|action| ActionRequest {
            action,
            source: controller,
        }));
        if pad.pointer_motion != Vec2::ZERO {
            pointer_owner = Some(controller);
        }
    }
    if let Some(controller) = pointer_owner {
        source.set_if_neq(ActiveInputDevice {
            source: controller,
            changed_at: time.elapsed(),
        });
    }
    if matches!(source.source, ActionSource::Controller(entity) if !pads.contains(entity)) {
        axes.set_if_neq(DeviceControlAxes::default());
        source.source = ActionSource::KeyboardMouse;
    }
}

use bevy::{
    camera::RenderTarget,
    input::mouse::MouseButtonInput,
    picking::pointer::{
        Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation,
        PointerPressState,
    },
    prelude::*,
    window::{PrimaryWindow, WindowRef},
};
use openzt2_interface_state::input::input_types::{ActionSource, ActiveInputDevice};

use super::gamepad_input_translation::GamepadControlSample;
use super::input_execution_types::DeviceControlAxes;
use super::keyboard_and_mouse_input_translation::KeyboardMouseControlSample;

pub(super) fn publish_active_device_axes_and_pointer(
    time: Res<Time<Real>>,
    keyboard: Query<&KeyboardMouseControlSample>,
    gamepads: Query<(Entity, &GamepadControlSample), With<Gamepad>>,
    (mut active, mut axes): (ResMut<ActiveInputDevice>, ResMut<DeviceControlAxes>),
    (physical_buttons, mut mouse_inputs): (
        Res<ButtonInput<MouseButton>>,
        MessageReader<MouseButtonInput>,
    ),
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut pointer: Query<(&PointerId, &mut PointerLocation, &PointerPressState)>,
    mut events: MessageWriter<PointerInput>,
) {
    let mut next_axes = DeviceControlAxes::default();
    let mut meaningful_device = None;
    if let Ok(keyboard) = keyboard.single() {
        next_axes = keyboard.axes;
        if keyboard.meaningful {
            meaningful_device = Some(ActionSource::KeyboardMouse);
        }
    }
    let mut pointer_owner = None;
    for (entity, pad) in &gamepads {
        if pad.meaningful {
            meaningful_device = Some(ActionSource::Controller(entity));
            next_axes = pad.axes;
        }
        if pad.pointer_motion != Vec2::ZERO {
            pointer_owner = Some(ActionSource::Controller(entity));
        }
    }
    let mut next_device = *active;
    if let Some(source) = pointer_owner.or(meaningful_device) {
        next_device = ActiveInputDevice {
            source,
            changed_at: time.elapsed(),
        };
    }
    let controller = match next_device.source {
        ActionSource::Controller(entity) => match gamepads.get(entity) {
            Ok((_, controller)) => Some(controller),
            Err(_) => {
                next_device.source = ActionSource::KeyboardMouse;
                next_axes = DeviceControlAxes::default();
                None
            }
        },
        _ => None,
    };
    active.set_if_neq(next_device);

    if let (Ok((window_entity, window)), Some((_, previous_location, held))) = (
        windows.single_mut(),
        pointer.iter_mut().find(|(id, ..)| id.is_mouse()),
    ) {
        if let Some(look) = publish_pointer_events(
            (window_entity, window),
            previous_location,
            held,
            controller,
            time.delta_secs(),
            &physical_buttons,
            mouse_inputs.read(),
            &mut events,
        ) {
            next_axes.look = look;
        }
    } else {
        mouse_inputs.clear();
    }
    axes.set_if_neq(next_axes);
}

fn publish_pointer_events<'a>(
    (window_entity, mut window): (Entity, Mut<Window>),
    mut previous_location: Mut<PointerLocation>,
    held: &PointerPressState,
    controller: Option<&GamepadControlSample>,
    elapsed_seconds: f32,
    physical_buttons: &ButtonInput<MouseButton>,
    mouse_inputs: impl Iterator<Item = &'a MouseButtonInput>,
    events: &mut MessageWriter<PointerInput>,
) -> Option<Vec2> {
    let target =
        RenderTarget::Window(WindowRef::Entity(window_entity)).normalize(Some(window_entity))?;
    let previous = previous_location
        .location()
        .map(|location| location.position);
    let physical = window.cursor_position();
    let mut look = None;
    let position = if let Some(controller) = controller {
        let current = physical
            .or(previous)
            .unwrap_or(Vec2::new(window.width(), window.height()) * 0.5);
        let next = (current + controller.pointer_motion * (600.0 * elapsed_seconds)).clamp(
            Vec2::ZERO,
            Vec2::new(
                window.physical_width().saturating_sub(1) as f32,
                window.physical_height().saturating_sub(1) as f32,
            ) / window.scale_factor(),
        );
        if controller.secondary {
            look = Some(((next - current) / 32.0).clamp(Vec2::splat(-1.0), Vec2::ONE));
        }
        if physical != Some(next) {
            window.set_cursor_position(Some(next));
        }
        Some(next)
    } else {
        physical
    };
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
    publish_pointer_button_events(
        &location,
        held,
        controller,
        physical_buttons,
        mouse_inputs,
        events,
    );
    look
}

fn publish_pointer_button_events<'a>(
    location: &Location,
    held: &PointerPressState,
    controller: Option<&GamepadControlSample>,
    physical_buttons: &ButtonInput<MouseButton>,
    mouse_inputs: impl Iterator<Item = &'a MouseButtonInput>,
    events: &mut MessageWriter<PointerInput>,
) {
    let mut pointer_buttons = [
        (
            MouseButton::Left,
            PointerButton::Primary,
            held.is_primary_pressed(),
            controller.is_some_and(|controller| controller.primary),
        ),
        (
            MouseButton::Right,
            PointerButton::Secondary,
            held.is_secondary_pressed(),
            controller.is_some_and(|controller| controller.secondary),
        ),
        (
            MouseButton::Middle,
            PointerButton::Middle,
            held.is_middle_pressed(),
            false,
        ),
    ];
    let mut write_transition = |button, held: &mut bool, pressed| {
        if *held != pressed {
            *held = pressed;
            let action = if pressed {
                PointerAction::Press(button)
            } else {
                PointerAction::Release(button)
            };
            events.write(PointerInput::new(
                PointerId::Mouse,
                location.clone(),
                action,
            ));
        }
    };

    // A physical press and release can both arrive before picking runs.
    for input in mouse_inputs {
        if let Some((_, button, held, controller_pressed)) = pointer_buttons
            .iter_mut()
            .find(|(mouse_button, ..)| *mouse_button == input.button)
        {
            write_transition(
                *button,
                held,
                *controller_pressed || input.state.is_pressed(),
            );
        }
    }

    // Controller changes use the combined held state of the shared pointer.
    for (mouse_button, button, mut held, controller_pressed) in pointer_buttons {
        write_transition(
            button,
            &mut held,
            controller_pressed || physical_buttons.pressed(mouse_button),
        );
    }
}

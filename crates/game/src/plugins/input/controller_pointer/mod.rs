//! Software pointer driven by generic gamepad actions through Bevy picking.

use bevy::{
    camera::RenderTarget,
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    prelude::*,
    window::{PrimaryWindow, WindowRef},
};

use super::{
    gamepad_input_translation::{apply_radial_gamepad_dead_zone, game_action_is_held},
    input_types::{
        ActionSource, ActiveInputDevice, GameAction, GameActionInputBindings,
        PrimaryPointerInputState,
    },
};

pub(super) fn project_controller_pointer(
    time: Res<Time<Real>>,
    gamepads: Query<(Entity, &Gamepad)>,
    bindings: Res<GameActionInputBindings>,
    mut active: ResMut<ActiveInputDevice>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut pointer: ResMut<PrimaryPointerInputState>,
    mut axes: ResMut<super::input_types::DeviceControlAxes>,
    mut events: MessageWriter<PointerInput>,
    mut pressed: Local<[bool; 2]>,
    mut previous_controller: Local<Option<Entity>>,
    phase: Res<State<crate::application_lifecycle::GamePhase>>,
    capture: Res<crate::plugins::ui::picking::UiPointerCapture>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
) {
    let Ok((window_entity, mut window)) = windows.single_mut() else {
        return;
    };
    let Some(target) =
        RenderTarget::Window(WindowRef::Entity(window_entity)).normalize(Some(window_entity))
    else {
        return;
    };
    for (entity, gamepad) in &gamepads {
        let axis = apply_radial_gamepad_dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
            -gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
        ));
        if axis != Vec2::ZERO {
            active.source = ActionSource::Controller(entity);
            active.changed_at = time.elapsed();
        }
    }
    let controller = match active.source {
        ActionSource::Controller(entity) => gamepads.get(entity).ok(),
        _ => None,
    };
    let axis = controller.map_or(Vec2::ZERO, |(_, gamepad)| {
        apply_radial_gamepad_dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
            -gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
        ))
    });
    let scale = window.scale_factor();
    let previous = if pointer.available {
        pointer.screen / scale
    } else {
        Vec2::new(window.width(), window.height()) * 0.5
    };
    let acquired =
        !pointer.available || *previous_controller != controller.map(|(entity, _)| entity);
    let position = (previous + axis * 600.0 * time.delta_secs()).clamp(
        Vec2::ZERO,
        Vec2::new(
            window.physical_width().saturating_sub(1) as f32,
            window.physical_height().saturating_sub(1) as f32,
        ) / scale,
    );
    let ui_navigation = *phase.get() != crate::application_lifecycle::GamePhase::InGame
        || capture.over_ui
        || modal_input.0.is_some();
    let next = controller.map_or([false; 2], |(_, gamepad)| {
        [
            game_action_is_held(GameAction::PrimaryPointer, &bindings, gamepad),
            !ui_navigation && game_action_is_held(GameAction::SecondaryPointer, &bindings, gamepad),
        ]
    });
    let location = Location { target, position };
    if controller.is_some() {
        pointer.screen = position * scale;
        pointer.available = true;
        pointer.pressed |= next[0];
        if next[1] {
            axes.look = ((position - previous) / 32.0).clamp(Vec2::splat(-1.0), Vec2::ONE);
        }
        if position != previous || acquired {
            window.set_cursor_position(Some(position));
            events.write(PointerInput::new(
                PointerId::Mouse,
                location.clone(),
                PointerAction::Move {
                    delta: position - previous,
                },
            ));
        }
    }
    for (index, button) in [PointerButton::Primary, PointerButton::Secondary]
        .into_iter()
        .enumerate()
    {
        if next[index] != pressed[index] {
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
    *pressed = next;
    *previous_controller = controller.map(|(entity, _)| entity);
}

pub(super) fn finalize_primary_pointer_edges(
    mut pointer: ResMut<PrimaryPointerInputState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut previous: Local<Option<(bool, Vec2, bool)>>,
) {
    let (held, position, available) = previous.unwrap_or((false, pointer.screen, false));
    pointer.just_pressed = pointer.pressed && !held;
    pointer.just_released = !pointer.pressed && held;
    pointer.delta = if available && pointer.available {
        (pointer.screen - position) / windows.single().map_or(1.0, Window::scale_factor)
    } else {
        Vec2::ZERO
    };
    *previous = Some((pointer.pressed, pointer.screen, pointer.available));
}

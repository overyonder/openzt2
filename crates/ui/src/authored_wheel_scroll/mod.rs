use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};

use super::authored_wheel_scroll_diagnostic_types::AuthoredWheelScrollConsumption;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_interface_state::ui::pointer_capture_types::{UiWheelCapture, WorldWheelInput};

const PIXELS_PER_WHEEL_LINE: f32 = 28.0;

/// Picking resolves one wheel receiver independently of click hover.
pub fn scroll_captured_authored_list_or_window_from_mouse_wheel(
    mut wheel: MessageReader<MouseWheel>,
    primary_windows: Query<Entity, With<PrimaryWindow>>,
    capture: Res<UiPointerCapture>,
    mut panels: Query<(&mut ScrollPosition, &ComputedNode, &Node)>,
    mut diagnostic: MessageWriter<AuthoredWheelScrollConsumption>,
    mut world: MessageWriter<WorldWheelInput>,
) {
    let Ok(window) = primary_windows.single() else {
        wheel.clear();
        return;
    };
    // The captured scroll rectangle can receive wheel input through empty cells.
    let mut event_count = 0usize;
    let mut first_event = None;
    let mut notches = 0.0;
    let mut sent_world_input = false;
    for event in wheel.read().filter(|event| event.window == window) {
        event_count += 1;
        first_event.get_or_insert((event.window, event.unit, event.x, event.y));
        let amount = match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / PIXELS_PER_WHEEL_LINE,
        };
        notches += amount;
        if capture.wheel == UiWheelCapture::World && amount != 0.0 {
            world.write(WorldWheelInput {
                window,
                notches: amount,
            });
            sent_world_input = true;
        }
    }
    if event_count == 0 {
        return;
    }
    let delta = -notches * PIXELS_PER_WHEEL_LINE;
    let mut record = |applied, outcome| {
        diagnostic.write(AuthoredWheelScrollConsumption {
            event_count,
            first_event,
            receiver: capture.wheel,
            delta,
            applied,
            outcome,
        });
    };
    if sent_world_input {
        record(None, "world-wheel");
        return;
    }
    if delta == 0.0 {
        record(None, "zero-delta");
        return;
    }
    let UiWheelCapture::Scroll(panel) = capture.wheel else {
        record(None, "no-scroll-receiver");
        return;
    };
    let Ok((mut position, computed, node)) = panels.get_mut(panel) else {
        record(None, "receiver-unavailable");
        return;
    };
    let maximum = (computed.content_size() - computed.size()).max(Vec2::ZERO)
        * computed.inverse_scale_factor();
    let before = position.0;
    // Two-row purchase lists pack across columns. Follow the overflow
    // axis projected from their authored grid rather than scrolling the
    // clipped vertical axis.
    let mut next = before;
    if node.overflow.x == OverflowAxis::Scroll && node.overflow.y != OverflowAxis::Scroll {
        next.x = (next.x + delta).clamp(0.0, maximum.x);
    } else {
        next.y = (next.y + delta).clamp(0.0, maximum.y);
    }
    position
        .reborrow()
        .map_unchanged(|position| &mut position.0)
        .set_if_neq(next);
    record(
        Some((
            before,
            position.0,
            maximum,
            node.overflow.x,
            node.overflow.y,
        )),
        "clamp-applied",
    );
}

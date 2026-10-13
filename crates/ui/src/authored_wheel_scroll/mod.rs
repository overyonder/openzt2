use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
};

use super::authored_wheel_scroll_diagnostic_types::AuthoredWheelScrollConsumption;
use super::authored_wheel_scroll_diagnostic_types::AuthoredWheelScrollDiagnosticState;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture;

/// Picking resolves one wheel receiver independently of click hover.
pub fn scroll_captured_authored_list_or_window_from_mouse_wheel(
    mut wheel: MessageReader<MouseWheel>,
    capture: Res<UiPointerCapture>,
    mut panels: Query<(&mut ScrollPosition, &ComputedNode, &Node)>,
    mut diagnostic: Option<ResMut<AuthoredWheelScrollDiagnosticState>>,
) {
    let record_input = diagnostic.is_some();
    let mut event_count = 0usize;
    let mut first_event = None;
    let delta = wheel.read().fold(0.0, |total, event| {
        if record_input {
            event_count += 1;
            if first_event.is_none() {
                first_event = Some((event.window, event.unit, event.x, event.y));
            }
        }
        total
            - event.y
                * if event.unit == MouseScrollUnit::Line {
                    28.0
                } else {
                    1.0
                }
    });
    let mut record = |applied, outcome| {
        if let Some(diagnostic) = diagnostic.as_deref_mut() {
            diagnostic.record_consumption(AuthoredWheelScrollConsumption {
                event_count,
                first_event,
                receiver: capture.wheel,
                delta,
                applied,
                outcome,
            });
        }
    };
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
    if node.overflow.x == OverflowAxis::Scroll && node.overflow.y != OverflowAxis::Scroll {
        position.x = (position.x + delta).clamp(0.0, maximum.x);
    } else {
        position.y = (position.y + delta).clamp(0.0, maximum.y);
    }
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

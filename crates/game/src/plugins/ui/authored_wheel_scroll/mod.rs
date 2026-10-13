use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
};

use super::picking::{UiPointerCapture, UiWheelCapture};
use crate::verification::verification_catalogue_wheel_diagnostics::CatalogueWheelDiagnostic;

/// Picking resolves one wheel receiver independently of click hover.
pub(super) fn scroll_captured_authored_list_or_window_from_mouse_wheel(
    mut wheel: MessageReader<MouseWheel>,
    capture: Res<UiPointerCapture>,
    mut panels: Query<(&mut ScrollPosition, &ComputedNode, &Node)>,
    mut diagnostic: Option<ResMut<CatalogueWheelDiagnostic>>,
) {
    let record_input = diagnostic
        .as_ref()
        .is_some_and(|diagnostic| diagnostic.awaiting_consumer());
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
    if record_input {
        info!(target: "openzt2_verification", event_count, ?first_event, delta,
            event_sample_limit = 1, events_truncated = event_count.saturating_sub(1),
            "catalogue wheel actual input; sample is window, unit, x, y");
    }
    if delta == 0.0 {
        if let Some(diagnostic) = diagnostic.as_deref_mut() {
            diagnostic.record_consumer(capture.wheel, delta, None, "zero-delta");
        }
        return;
    }
    let UiWheelCapture::Scroll(panel) = capture.wheel else {
        if let Some(diagnostic) = diagnostic.as_deref_mut() {
            diagnostic.record_consumer(capture.wheel, delta, None, "no-scroll-receiver");
        }
        return;
    };
    let Ok((mut position, computed, node)) = panels.get_mut(panel) else {
        if let Some(diagnostic) = diagnostic.as_deref_mut() {
            diagnostic.record_consumer(capture.wheel, delta, None, "receiver-unavailable");
        }
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
    if let Some(diagnostic) = diagnostic.as_deref_mut() {
        diagnostic.record_consumer(
            capture.wheel,
            delta,
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
}

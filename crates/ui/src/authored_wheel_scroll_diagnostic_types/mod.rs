use bevy::{input::mouse::MouseScrollUnit, prelude::*};

use openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture;

/// The latest wheel scroll consumption, installed only by runtime verification.
#[derive(Message, Debug, Clone, Copy)]
pub struct AuthoredWheelScrollConsumption {
    pub event_count: usize,
    /// Window, unit and offsets of the first wheel event.
    pub first_event: Option<(Entity, MouseScrollUnit, f32, f32)>,
    pub receiver: UiWheelCapture,
    pub delta: f32,
    /// Scroll position before and after, maximum, and the overflow axes.
    pub applied: Option<(Vec2, Vec2, Vec2, OverflowAxis, OverflowAxis)>,
    pub outcome: &'static str,
}

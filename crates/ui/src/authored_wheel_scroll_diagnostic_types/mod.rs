use bevy::{input::mouse::MouseScrollUnit, prelude::*};

use openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture;

/// The latest wheel scroll consumption, installed only by runtime verification.
#[derive(Resource, Default)]
pub struct AuthoredWheelScrollDiagnosticState {
    pending: Option<AuthoredWheelScrollConsumption>,
}

impl AuthoredWheelScrollDiagnosticState {
    pub(super) const fn record_consumption(&mut self, consumption: AuthoredWheelScrollConsumption) {
        self.pending = Some(consumption);
    }

    pub const fn take_pending_consumption(&mut self) -> Option<AuthoredWheelScrollConsumption> {
        self.pending.take()
    }
}

/// What the captured wheel receiver did with one frame of wheel input.
#[derive(Clone, Copy)]
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

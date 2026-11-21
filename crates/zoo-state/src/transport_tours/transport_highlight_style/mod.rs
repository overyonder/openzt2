use std::time::Duration;

use bevy::prelude::*;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TransportHighlightStyle {
    pub start: [Color; 2],
    pub target: Color,
    pub invalid: Color,
    pub steep: Color,
    pub open_end: [Color; 2],
    pub delete: [Color; 2],
    pub blink_period: Duration,
    pub deletion_blink_period: Duration,
}

impl Default for TransportHighlightStyle {
    fn default() -> Self {
        Self {
            start: [Color::srgb(0.0, 0.0, 1.0); 2],
            target: Color::srgb(0.0, 1.0, 0.0),
            invalid: Color::srgb_u8(200, 20, 10),
            steep: Color::srgb(1.0, 1.0, 0.0),
            open_end: [Color::srgb(0.0, 0.0, 1.0), Color::WHITE],
            delete: [Color::srgb(1.0, 0.0, 0.0), Color::srgb_u8(255, 128, 0)],
            blink_period: Duration::from_secs(1),
            deletion_blink_period: Duration::from_secs(1),
        }
    }
}

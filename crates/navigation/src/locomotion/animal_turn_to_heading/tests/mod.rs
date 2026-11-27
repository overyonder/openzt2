use super::{heading_error, turn_clip};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[test]
fn a_turn_takes_quarter_turn_clips_then_the_shuffle_until_within_the_error() {
    let tolerance = 5.0_f32.to_radians();
    // Facing +Z, a heading towards +X lies a quarter turn to the left.
    let error = heading_error(Quat::IDENTITY, FRAC_PI_2);
    assert!((error - FRAC_PI_2).abs() < 1.0e-5, "{error}");
    assert_eq!(turn_clip(error, tolerance), Some("Stand_TurnLeft90"));
    // A heading behind is a half turn away, taken a quarter at a time.
    assert!((heading_error(Quat::IDENTITY, PI - 0.01).abs() - PI).abs() < 0.02);
    assert_eq!(turn_clip(-FRAC_PI_2, tolerance), Some("Stand_TurnRight90"));
    assert_eq!(turn_clip(0.3, tolerance), Some("Stand_ShuffleGeneric"));
    assert_eq!(turn_clip(-0.3, tolerance), Some("Stand_ShuffleGeneric"));
    assert_eq!(turn_clip(0.05, tolerance), None);
}

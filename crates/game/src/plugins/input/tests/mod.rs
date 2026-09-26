use bevy::prelude::*;

use crate::plugins::input::input_types::GameAction;

use super::{
    gamepad_input_translation::apply_radial_gamepad_dead_zone,
    input_types::{GameActionInputBindings, InputChord},
};

#[test]
fn original_wasd_and_arrow_pan_aliases_are_both_bound() {
    let bindings = GameActionInputBindings::default();
    let up = bindings
        .entries
        .iter()
        .find(|binding| binding.action == GameAction::NavigateUp)
        .expect("up binding");
    assert_eq!(
        up.primary,
        InputChord::KeyPair(KeyCode::KeyW, KeyCode::ArrowUp)
    );
}

#[test]
fn dead_zone_rejects_noise_and_preserves_analog_magnitude() {
    assert_eq!(
        apply_radial_gamepad_dead_zone(Vec2::new(0.05, -0.05)),
        Vec2::ZERO
    );
    let output = apply_radial_gamepad_dead_zone(Vec2::new(0.6, 0.0));
    assert!(output.x > 0.0 && output.x < 1.0);
    assert_eq!(output.y, 0.0);
}

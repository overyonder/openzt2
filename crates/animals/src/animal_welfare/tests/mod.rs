use super::{
    calculations::{
        animal_need_crossed_authored_threshold, clamp_animal_need_q16_after_delta,
        welfare_from_need_points,
    },
    types::*,
};

#[test]
fn q16_decay_preserves_fractional_ticks() {
    let mut value = 1000 * Q16_ONE;
    let rate = -(Q16_ONE / 4);
    for _ in 0..3 {
        value = clamp_animal_need_q16_after_delta(value, rate);
    }
    assert_eq!(value, 999 * Q16_ONE + Q16_ONE / 4);
    value = clamp_animal_need_q16_after_delta(value, rate);
    assert_eq!(value, 999 * Q16_ONE);
}

#[test]
fn need_messages_are_threshold_edge_triggered() {
    assert!(!animal_need_crossed_authored_threshold(
        401 * Q16_ONE,
        400 * Q16_ONE,
        Some(200),
        Some(700)
    ));
    assert!(animal_need_crossed_authored_threshold(
        201 * Q16_ONE,
        200 * Q16_ONE,
        Some(200),
        Some(700)
    ));
    assert!(!animal_need_crossed_authored_threshold(
        199 * Q16_ONE,
        150 * Q16_ONE,
        Some(200),
        Some(700)
    ));
    assert!(animal_need_crossed_authored_threshold(
        699 * Q16_ONE,
        700 * Q16_ONE,
        Some(200),
        Some(700)
    ));
}

#[test]
fn welfare_is_the_need_point_balance() {
    let points = |good, bad| NeedPoints {
        good,
        bad,
        seconds_until_update: 0.0,
    };
    assert_eq!(
        welfare_from_need_points(&points(0.0, 0.0)),
        (500, WelfareBand::Poor)
    );
    assert_eq!(
        welfare_from_need_points(&points(10.0, 0.0)),
        (550, WelfareBand::Good)
    );
    assert_eq!(
        welfare_from_need_points(&points(0.0, 49.8)),
        (251, WelfareBand::Poor)
    );
    assert_eq!(
        welfare_from_need_points(&points(0.0, 50.0)),
        (250, WelfareBand::Critical)
    );
    assert_eq!(
        welfare_from_need_points(&points(100.0, 0.0)),
        (1000, WelfareBand::Good)
    );
    assert_eq!(
        welfare_from_need_points(&points(0.0, 100.0)),
        (0, WelfareBand::Critical)
    );
}

#[test]
fn need_points_grow_on_one_branch_and_shrink_the_other() {
    let mut points = NeedPoints::default();
    // 150 updates of 3 s with an unmet need reach a fence bash's 30.
    for _ in 0..150 {
        super::systems::step_need_points(&mut points, true);
    }
    assert!((points.bad - 30.0).abs() < 1.0e-3);
    assert_eq!(points.good, 0.0);
    // A met need takes back one update's worth, not the whole spell.
    super::systems::step_need_points(&mut points, false);
    assert!((points.bad - 29.8).abs() < 1.0e-3);
    assert!((points.good - 0.2).abs() < 1.0e-6);
    for _ in 0..1000 {
        super::systems::step_need_points(&mut points, false);
    }
    assert_eq!(points.good, 100.0);
    assert_eq!(points.bad, 0.0);
}

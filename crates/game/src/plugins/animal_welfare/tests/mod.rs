use super::{
    calculations::{
        animal_need_crossed_authored_threshold, calculate_mean_animal_welfare_with_habitat_limit,
        clamp_animal_need_q16_after_delta,
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
fn welfare_uses_canonical_need_mean_and_habitat_cap() {
    let mut values = [800 * Q16_ONE; 10];
    values[0] = 200 * Q16_ONE;
    let habitat = HabitatSuitability {
        space: 1000,
        biome: 1000,
        overall: 900,
    };
    assert_eq!(
        calculate_mean_animal_welfare_with_habitat_limit(values, habitat),
        740
    );
    let capped = HabitatSuitability {
        overall: 300,
        ..habitat
    };
    assert_eq!(
        calculate_mean_animal_welfare_with_habitat_limit(values, capped),
        300
    );
}

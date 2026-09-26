use super::types::{HabitatSuitability, WelfareBand, MAX_NEED_Q16, Q16_ONE};

#[inline]
pub(super) fn clamp_animal_need_q16_after_delta(value: i32, delta_q16: i32) -> i32 {
    value.saturating_add(delta_q16).clamp(0, MAX_NEED_Q16)
}

#[inline]
pub(super) fn convert_animal_need_q16_to_permille(value_q16: i32) -> u16 {
    (value_q16.clamp(0, MAX_NEED_Q16) / Q16_ONE) as u16
}

#[inline]
pub(super) fn animal_need_crossed_authored_threshold(
    old_q16: i32,
    new_q16: i32,
    trigger: Option<u16>,
    cessation: Option<u16>,
) -> bool {
    let old = convert_animal_need_q16_to_permille(old_q16);
    let new = convert_animal_need_q16_to_permille(new_q16);
    trigger.is_some_and(|threshold| old > threshold && new <= threshold)
        || cessation.is_some_and(|threshold| old < threshold && new >= threshold)
}

pub(super) fn calculate_mean_animal_welfare_with_habitat_limit(
    values_q16: [i32; 10],
    habitat: HabitatSuitability,
) -> u16 {
    let needs = values_q16
        .into_iter()
        .map(convert_animal_need_q16_to_permille)
        .fold(0_u32, |sum, value| sum + u32::from(value))
        / values_q16.len() as u32;
    // Habitat is a hard welfare constraint: excellent individual needs cannot
    // conceal an unsuitable or breached enclosure.
    needs.min(u32::from(habitat.overall)) as u16
}

pub(super) fn classify_animal_welfare_against_authored_thresholds(
    value: u16,
    critical: u16,
    good: u16,
) -> WelfareBand {
    let critical = critical.min(1000);
    let good = good.clamp(critical, 1000);
    let fair = critical + (good - critical) / 2;
    let excellent = good + (1000 - good) / 2;
    match value {
        value if value <= critical => WelfareBand::Critical,
        value if value < fair => WelfareBand::Poor,
        value if value < good => WelfareBand::Fair,
        value if value < excellent => WelfareBand::Good,
        _ => WelfareBand::Excellent,
    }
}

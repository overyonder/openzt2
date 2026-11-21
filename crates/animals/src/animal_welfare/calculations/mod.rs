use super::types::{NeedPoints, WelfareBand, MAX_NEED_Q16, Q16_ONE};

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

/// The animal's welfare as the original's needs panel shows it: bad need
/// points minus good ones, -100 to 100. Below 0 the animal is content, from 0
/// its needs are pressing and from half the maximum critical. Welfare is that
/// balance as permille, 1000 for -100 and 0 for 100.
pub(super) fn welfare_from_need_points(points: &NeedPoints) -> (u16, WelfareBand) {
    let balance = points.bad - points.good;
    let permille = ((100.0 - balance) * 5.0).round().clamp(0.0, 1000.0) as u16;
    let band = if balance >= 50.0 {
        WelfareBand::Critical
    } else if balance >= 0.0 {
        WelfareBand::Poor
    } else {
        WelfareBand::Good
    };
    (permille, band)
}

const MAXIMUM_DISEASE_SEVERITY_PERMILLE: u16 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AdvancedDiseaseTick {
    pub elapsed_ticks: u64,
    pub severity_permille: u16,
    pub hint_level: u8,
    pub vitality_delta_permille: i32,
    pub fatal: bool,
}

pub(super) fn advance_disease_severity_hint_and_vitality_by_one_tick(
    elapsed_ticks: u64,
    severity_permille: u16,
    severity_per_tick_q16: i32,
    vitality_per_tick_q16: i32,
    fatal_threshold: u16,
    hint_thresholds: [u16; 3],
) -> AdvancedDiseaseTick {
    let next_elapsed_tick = elapsed_ticks.saturating_add(1);
    let severity_delta = calculate_q16_accumulated_delta_between_ticks(
        elapsed_ticks,
        next_elapsed_tick,
        severity_per_tick_q16,
    );
    let severity_permille = (i64::from(severity_permille) + severity_delta)
        .clamp(0, i64::from(MAXIMUM_DISEASE_SEVERITY_PERMILLE)) as u16;
    let vitality_delta_permille = calculate_q16_accumulated_delta_between_ticks(
        elapsed_ticks,
        next_elapsed_tick,
        vitality_per_tick_q16,
    )
    .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let hint_level = hint_thresholds
        .iter()
        .take_while(|threshold| severity_permille >= **threshold)
        .count() as u8;

    AdvancedDiseaseTick {
        elapsed_ticks: next_elapsed_tick,
        severity_permille,
        hint_level,
        vitality_delta_permille,
        fatal: fatal_threshold <= MAXIMUM_DISEASE_SEVERITY_PERMILLE
            && severity_permille >= fatal_threshold,
    }
}

pub(super) fn apply_treatment_delta_to_disease_severity(
    severity_permille: u16,
    delta_permille: i16,
) -> u16 {
    (i32::from(severity_permille) + i32::from(delta_permille))
        .clamp(0, i32::from(MAXIMUM_DISEASE_SEVERITY_PERMILLE)) as u16
}

fn calculate_q16_accumulated_delta_between_ticks(
    first_tick: u64,
    second_tick: u64,
    rate_q16: i32,
) -> i64 {
    let rate_q16 = i128::from(rate_q16);
    let first_accumulated_value = i128::from(first_tick) * rate_q16;
    let second_accumulated_value = i128::from(second_tick) * rate_q16;
    ((second_accumulated_value >> 16) - (first_accumulated_value >> 16))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q16_progress_preserves_fractional_ticks() {
        let first = advance_disease_severity_hint_and_vitality_by_one_tick(
            0,
            0,
            32_768,
            -32_768,
            1_000,
            [1, 2, 3],
        );
        let second = advance_disease_severity_hint_and_vitality_by_one_tick(
            first.elapsed_ticks,
            first.severity_permille,
            32_768,
            -32_768,
            1_000,
            [1, 2, 3],
        );
        assert_eq!(first.severity_permille, 0);
        assert_eq!(second.severity_permille, 1);
        assert_eq!(first.vitality_delta_permille, -1);
        assert_eq!(second.vitality_delta_permille, 0);
    }
}

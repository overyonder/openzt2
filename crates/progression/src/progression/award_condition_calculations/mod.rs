pub(crate) fn advance_award_condition_satisfied_tick_count(
    current_satisfied_ticks: u64,
    condition_is_satisfied: bool,
    required_satisfied_ticks: u64,
) -> u64 {
    if condition_is_satisfied {
        current_satisfied_ticks
            .saturating_add(1)
            .min(required_satisfied_ticks)
    } else {
        0
    }
}

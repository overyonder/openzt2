use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

pub(super) fn sample_inclusive_u32_range(
    inclusive_range: [u32; 2],
    random: &mut DeterministicRng,
) -> u32 {
    let width = inclusive_range[1]
        .saturating_sub(inclusive_range[0])
        .saturating_add(1);
    inclusive_range[0].saturating_add(random.range_u32(width).unwrap_or(0))
}

pub(super) fn random_probability_threshold_passes(
    probability_threshold: u32,
    random: &mut DeterministicRng,
) -> bool {
    probability_threshold == u32::MAX
        || (probability_threshold != 0 && random.next_u32() < probability_threshold)
}

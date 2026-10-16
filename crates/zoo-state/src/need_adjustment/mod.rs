use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

pub fn advance_need_adjustment_timer(
    remaining_ns: &mut u64,
    step_ns: u64,
    delay_steps: u32,
    rng: &mut DeterministicRng,
) -> bool {
    *remaining_ns = remaining_ns.saturating_sub(step_ns);
    if *remaining_ns != 0 {
        return false;
    }
    let Some(delay) = rng.range_u32(delay_steps) else {
        return false;
    };
    // The native thinker chooses [0, 2 * StateThinkerInterval) and
    // multiplies by 100 / 1000 seconds. A zero delay expires next tick.
    *remaining_ns = u64::from(delay) * 100_000_000;
    true
}

#[cfg(test)]
mod need_adjustment_timer_tests {
    use super::*;

    #[test]
    fn need_adjustment_waits_without_consuming_randomness() {
        let mut rng = DeterministicRng::from_raw([42, 3]);
        let original_rng = rng;
        let mut remaining = 300_000_000;
        for _ in 0..2 {
            assert!(!advance_need_adjustment_timer(
                &mut remaining,
                100_000_000,
                60,
                &mut rng,
            ));
            assert_eq!(rng, original_rng);
        }
        assert!(advance_need_adjustment_timer(
            &mut remaining,
            100_000_000,
            60,
            &mut rng,
        ));
        assert!(remaining < 6_000_000_000);
        assert_eq!(remaining % 100_000_000, 0);
        assert_ne!(rng, original_rng);
    }
}

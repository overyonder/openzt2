use std::time::Duration;

use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

/// Fires at most once per thinker step, including when the next delay is zero.
pub fn tick_need_adjustment(
    timer: &mut Timer,
    step: Duration,
    delay_steps: u32,
    random: &mut DeterministicRng,
) -> bool {
    if timer.is_paused() || !timer.tick(step).is_finished() {
        return false;
    }
    let Some(delay) = random.range_u32(delay_steps) else {
        return false;
    };
    timer.set_duration(Duration::from_millis(u64::from(delay) * 100));
    timer.reset();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn need_adjustment_waits_without_consuming_randomness() {
        let mut rng = DeterministicRng::from_raw([42, 3]);
        let original_rng = rng;
        let mut timer = Timer::new(Duration::from_millis(300), TimerMode::Once);
        for _ in 0..2 {
            assert!(!tick_need_adjustment(
                &mut timer,
                Duration::from_millis(100),
                60,
                &mut rng,
            ));
            assert_eq!(rng, original_rng);
        }
        assert!(tick_need_adjustment(
            &mut timer,
            Duration::from_millis(100),
            60,
            &mut rng,
        ));
        assert!(timer.remaining() < Duration::from_secs(6));
        assert_eq!(timer.remaining().as_millis() % 100, 0);
        assert_ne!(rng, original_rng);
    }
}

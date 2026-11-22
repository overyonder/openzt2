use std::time::Duration;

use bevy::{prelude::*, time::Stopwatch};

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ScenarioScriptTiming {
    evaluation: Stopwatch,
    always_run: Stopwatch,
}

impl ScenarioScriptTiming {
    pub fn from_elapsed_seconds(evaluation: f32, always_run: f32) -> Option<Self> {
        let mut timing = Self::default();
        timing
            .evaluation
            .set_elapsed(Duration::try_from_secs_f32(evaluation).ok()?);
        timing
            .always_run
            .set_elapsed(Duration::try_from_secs_f32(always_run).ok()?);
        Some(timing)
    }

    pub fn elapsed_seconds(&self) -> [f32; 2] {
        [
            self.evaluation.elapsed_secs(),
            self.always_run.elapsed_secs(),
        ]
    }

    pub fn evaluation_is_due(&mut self, time: &Time<Fixed>, interval_seconds: f32) -> bool {
        advance_scenario_stopwatch(&mut self.evaluation, time, interval_seconds)
    }

    pub fn always_run_is_due(&mut self, time: &Time<Fixed>) -> bool {
        advance_scenario_stopwatch(&mut self.always_run, time, 3.0)
    }
}

fn advance_scenario_stopwatch(
    stopwatch: &mut Stopwatch,
    time: &Time<Fixed>,
    interval_seconds: f32,
) -> bool {
    // The interval comparison and each elapsed-time addition use single precision.
    let elapsed = stopwatch.elapsed_secs() + time.delta_secs();
    if elapsed > interval_seconds {
        stopwatch.reset();
        return true;
    }
    stopwatch.tick(time.delta());
    if stopwatch.elapsed_secs() != elapsed {
        stopwatch.set_elapsed(Duration::from_secs_f32(elapsed));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_schedules_cross_strictly_and_discard_overshoot() {
        let mut timing = ScenarioScriptTiming::default();
        let mut time = Time::<Fixed>::default();
        time.advance_by(Duration::from_secs(3));
        assert!(!timing.evaluation_is_due(&time, 4.0));
        assert!(!timing.always_run_is_due(&time));
        time.advance_by(Duration::from_secs(1));
        assert!(!timing.evaluation_is_due(&time, 4.0));
        assert!(timing.always_run_is_due(&time));
        time.advance_by(Duration::from_millis(250));
        assert!(timing.evaluation_is_due(&time, 4.0));
        assert!(!timing.always_run_is_due(&time));
        assert_eq!(timing.elapsed_seconds(), [0.0, 0.25]);
    }
}

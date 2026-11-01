use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ScenarioScriptTiming {
    pub evaluation_elapsed_seconds: f32,
    pub always_run_elapsed_seconds: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_schedules_cross_strictly_and_discard_overshoot() {
        let mut timing = ScenarioScriptTiming::default();
        assert_eq!(timing.advance(3.0, 4.0), (false, false));
        assert_eq!(timing.advance(1.0, 4.0), (false, true));
        assert_eq!(timing.advance(0.25, 4.0), (true, false));
        assert_eq!(timing.evaluation_elapsed_seconds, 0.0);
        assert_eq!(timing.always_run_elapsed_seconds, 0.25);
    }
}

impl ScenarioScriptTiming {
    pub fn advance(
        &mut self,
        delta_seconds: f32,
        evaluation_interval_seconds: f32,
    ) -> (bool, bool) {
        self.evaluation_elapsed_seconds += delta_seconds;
        self.always_run_elapsed_seconds += delta_seconds;
        let evaluate = self.evaluation_elapsed_seconds > evaluation_interval_seconds;
        let always_run = self.always_run_elapsed_seconds > 3.0;
        if evaluate {
            self.evaluation_elapsed_seconds = 0.0;
        }
        if always_run {
            self.always_run_elapsed_seconds = 0.0;
        }
        (evaluate, always_run)
    }
}

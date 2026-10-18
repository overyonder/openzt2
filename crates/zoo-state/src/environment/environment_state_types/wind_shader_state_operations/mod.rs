use bevy::prelude::*;

use super::{AuthoredWindShaderPresentationState, Wind, WindEnvelopeInterpolator};

impl WindEnvelopeInterpolator {
    fn new(minimum: f32, maximum: f32, stage_ranges: [[f32; 2]; 4]) -> Self {
        Self {
            elapsed: 0.0,
            duration: 0.0,
            attack: 0.0,
            hold: 0.0,
            release: 0.0,
            idle: 0.0,
            output: 0.0,
            random_amplitude: 0.0,
            minimum,
            maximum,
            stage_ranges,
        }
    }

    fn advance(&mut self, elapsed_seconds: f32, random_unit: f32) {
        self.elapsed += elapsed_seconds;
        if self.elapsed >= self.duration {
            let stages = self
                .stage_ranges
                .map(|[minimum, maximum]| minimum + random_unit * (maximum - minimum));
            [self.attack, self.hold, self.release, self.idle] = stages;
            self.duration = self.attack + self.hold + self.release + self.idle;
            self.elapsed = 0.0;
            self.random_amplitude = random_unit;
        }
        let envelope = if self.elapsed < self.attack {
            smoothstep(self.elapsed / self.attack)
        } else if self.elapsed - self.attack < self.hold {
            1.0
        } else if self.elapsed - self.attack - self.hold < self.release {
            smoothstep(1.0 - (self.elapsed - self.attack - self.hold) / self.release)
        } else {
            0.0
        };
        self.output =
            self.minimum + (self.maximum - self.minimum) * self.random_amplitude * envelope;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_draws_drive_envelopes_and_preserve_wrap_endpoints() {
        let mut state = AuthoredWindShaderPresentationState::from_current(45.0, 0.5);
        let mut draws = [0.5, 0.5, 0.5, 0.25, 0.5, 0.5].into_iter();
        state.advance_current(45.0, 0.5, 0.0, || draws.next().unwrap());
        state.advance_current(45.0, 0.5, 1.0, || draws.next().unwrap());

        // The shared half draw gives a two-second main attack and a
        // one-second vibration attack. At one second their outputs are
        // 1/2 + (1/2 * 1/2 * 1/2) and .01 * 1/2 respectively.
        assert_eq!(state.direction_envelope.output, 0.625);
        assert!((state.vibration_envelope.output - 0.005).abs() < 1e-7);
        assert_eq!(state.vibration_phase, 0.5);
        assert!(draws.next().is_none());
        let phases = state.shader_registers()[3];
        assert!((phases.x - 0.95).abs() < 1e-7);
        assert!((phases.y - 0.495).abs() < 1e-7);
        assert!((phases.z - 0.95).abs() < 1e-7);
        assert_eq!(phases.w, 1.0);
        assert_eq!(wrap_unit(1.0), 1.0);
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn wrap_unit(value: f32) -> f32 {
    if value > 1.0 {
        value - value.trunc()
    } else if value < 0.0 {
        value - value.trunc() + 1.0
    } else {
        value
    }
}

impl AuthoredWindShaderPresentationState {
    pub fn set_direction(&mut self, direction: Vec2) {
        self.direction = direction.normalize_or_zero();
    }

    pub fn from_wind(wind: Wind) -> Self {
        Self::new(wind.direction, wind.speed_mps, 0.5)
    }

    /// Current direction and shader vectors use source horizontal XY and Z-up.
    pub fn from_current(speed_mps: f32, gustiness: f32) -> Self {
        Self::new(Vec2::Y, speed_mps, gustiness)
    }

    fn new(direction: Vec2, speed_mps: f32, gustiness: f32) -> Self {
        let mut state = Self {
            direction: direction.normalize_or_zero(),
            speed_mps: 0.0,
            gustiness: 0.0,
            direction_envelope: WindEnvelopeInterpolator::new(
                0.0,
                0.0,
                [[1.0, 3.0], [0.0, 4.0], [1.0, 4.0], [0.0, 2.0]],
            ),
            vibration_envelope: WindEnvelopeInterpolator::new(
                0.0,
                0.0,
                [[0.5, 1.5], [0.0, 4.0], [1.5, 7.5], [0.0, 0.0]],
            ),
            vibration_phase: 0.0,
            texture_phases: Vec4::ZERO,
        };
        state.apply_configuration(speed_mps, gustiness);
        state
    }

    pub fn advance(&mut self, wind: Wind, elapsed_seconds: f32, random_unit: impl FnMut() -> f32) {
        self.advance_current(wind.speed_mps, self.gustiness, elapsed_seconds, random_unit);
    }

    pub fn advance_current(
        &mut self,
        speed_mps: f32,
        gustiness: f32,
        elapsed_seconds: f32,
        mut random_unit: impl FnMut() -> f32,
    ) {
        self.apply_configuration(speed_mps, gustiness);
        let envelope_random = random_unit();
        self.direction_envelope
            .advance(elapsed_seconds, envelope_random);
        self.vibration_envelope
            .advance(elapsed_seconds, envelope_random);
        let random_walk = Vec2::new(random_unit() * 2.0 - 1.0, random_unit() * 2.0 - 1.0)
            * (elapsed_seconds * 0.01);
        self.direction = (self.direction + random_walk).normalize_or_zero();
        let amplitude_range = self.vibration_envelope.maximum - self.vibration_envelope.minimum;
        let normalized_vibration = if amplitude_range == 0.0 {
            0.0
        } else {
            ((self.vibration_envelope.output - self.vibration_envelope.minimum) / amplitude_range)
                .clamp(0.0, 1.0)
        };
        self.vibration_phase =
            wrap_unit(self.vibration_phase + normalized_vibration * 5.0 * elapsed_seconds);
        let gust_phase_speed =
            (f64::from(0.01_f32) + f64::from(self.gustiness) * (1.0 - f64::from(0.01_f32))) as f32;
        self.texture_phases = (self.texture_phases
            - Vec4::new(0.05, gust_phase_speed, 0.05, 1.0) * elapsed_seconds)
            .map(wrap_unit);
    }

    fn apply_configuration(&mut self, speed_mps: f32, gustiness: f32) {
        self.speed_mps = speed_mps.clamp(0.0, 45.0);
        self.gustiness = gustiness.clamp(0.0, 1.0);
        let amplitude = (f64::from(0.025_f32)
            + f64::from(self.speed_mps / 45.0) * (1.0 - f64::from(0.025_f32)))
            as f32;
        self.direction_envelope.minimum = amplitude * 0.5;
        self.direction_envelope.maximum = amplitude;
        self.vibration_envelope.maximum = (f64::from(smoothstep(self.gustiness / 0.5))
            * f64::from(smoothstep(self.speed_mps / 15.0))
            * f64::from(0.01_f32)) as f32;
    }

    pub fn shader_registers(&self) -> [Vec4; 7] {
        let direction = self.direction;
        let sine = (self.vibration_phase * std::f32::consts::TAU).sin();
        let vibration = Vec3::new(-direction.y * sine, direction.x * sine, 0.0);
        let direction_3d = direction.extend(0.0);
        let perpendicular = direction_3d.cross(Vec3::Z);
        [
            (direction_3d * self.direction_envelope.output).extend(0.0),
            (vibration * self.vibration_envelope.output).extend(0.0),
            Vec4::new(0.1, 0.1, 0.1, 1.0),
            self.texture_phases,
            perpendicular.extend(0.0),
            direction_3d.extend(0.0),
            Vec3::Z.extend(0.0),
        ]
    }
}

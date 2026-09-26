use bevy::prelude::*;

use super::{AuthoredWindShaderPresentationState, Wind, WindEnvelopeInterpolator};

impl WindEnvelopeInterpolator {
    fn with_output_range(minimum: f32, maximum: f32) -> Self {
        Self {
            elapsed: 0.0,
            duration: 0.0,
            attack: 0.0,
            hold: 0.0,
            release: 0.0,
            idle: 0.0,
            output: minimum,
            random_amplitude: 0.0,
            minimum,
            maximum,
        }
    }

    fn advance(&mut self, elapsed_seconds: f32, random_unit: f32) -> f32 {
        self.elapsed += elapsed_seconds;
        if self.elapsed > self.duration {
            self.attack = 1.0 + random_unit;
            self.hold = random_unit;
            self.release = 2.0 + random_unit;
            self.idle = random_unit;
            self.duration = self.attack + self.hold + self.release + self.idle;
            self.elapsed = 0.0;
            self.random_amplitude = random_unit;
        }
        let envelope = if self.elapsed < self.attack {
            smoothstep(self.elapsed / self.attack)
        } else if self.elapsed < self.attack + self.hold {
            1.0
        } else if self.elapsed < self.attack + self.hold + self.release {
            smoothstep(1.0 - (self.elapsed - self.attack - self.hold) / self.release)
        } else {
            0.0
        };
        self.output =
            self.minimum + (self.maximum - self.minimum) * self.random_amplitude * envelope;
        self.output
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

impl AuthoredWindShaderPresentationState {
    pub(in crate::plugins::environment) fn from_wind(wind: Wind, random_state: u32) -> Self {
        let normalized_speed = (wind.speed_mps / 45.0).clamp(0.0, 1.0);
        let wind_amplitude = normalized_speed * 0.975 + 0.025;
        let direction_randomness = smoothstep((wind.speed_mps / 15.0).clamp(0.0, 1.0)) * 0.01;
        Self {
            direction: wind.direction.normalize_or_zero(),
            speed_mps: wind.speed_mps.clamp(0.0, 45.0),
            direction_envelope: WindEnvelopeInterpolator::with_output_range(
                wind_amplitude * 0.5,
                wind_amplitude,
            ),
            vibration_envelope: WindEnvelopeInterpolator::with_output_range(0.0, 1.0),
            direction_randomness,
            vibration_phase: 0.5,
            texture_phases: Vec4::ZERO,
            random_state,
        }
    }

    pub(in crate::plugins::environment) fn advance(&mut self, wind: Wind, elapsed_seconds: f32) {
        if self.speed_mps != wind.speed_mps {
            self.apply_authored_wind_speed(wind.speed_mps);
        }
        let target_direction = wind.direction.normalize_or_zero();
        let interpolation = (elapsed_seconds * 0.5).clamp(0.0, 1.0);
        self.direction = self
            .direction
            .lerp(target_direction, interpolation)
            .normalize_or_zero();

        let direction_envelope_random = self.next_unit_random();
        let vibration_envelope_random = self.next_unit_random();
        self.direction_envelope
            .advance(elapsed_seconds, direction_envelope_random);
        self.vibration_envelope
            .advance(elapsed_seconds, vibration_envelope_random);
        let random_rotation = Vec2::new(self.next_signed_random(), self.next_signed_random())
            * elapsed_seconds
            * std::f32::consts::TAU;
        self.direction =
            (self.direction + random_rotation * self.direction_randomness).normalize_or_zero();
        self.vibration_phase =
            (self.vibration_phase + elapsed_seconds * wind.speed_mps * 0.01).fract();
        self.texture_phases = (self.texture_phases
            - Vec4::new(0.1, 0.1, 0.1, 1.0) * elapsed_seconds)
            .map(|phase| phase.rem_euclid(1.0));
    }

    fn apply_authored_wind_speed(&mut self, speed_mps: f32) {
        self.speed_mps = speed_mps.clamp(0.0, 45.0);
        let normalized_speed = self.speed_mps / 45.0;
        let wind_amplitude = normalized_speed * 0.975 + 0.025;
        self.direction_envelope.minimum = wind_amplitude * 0.5;
        self.direction_envelope.maximum = wind_amplitude;
        self.direction_randomness = smoothstep((self.speed_mps / 15.0).clamp(0.0, 1.0)) * 0.01;
    }

    pub(crate) fn shader_registers(&self) -> [Vec4; 7] {
        let direction = self.direction;
        let vibration = Vec3::new(
            -direction.y * (self.vibration_phase * std::f32::consts::TAU).sin(),
            direction.x * (self.vibration_phase * std::f32::consts::TAU).sin(),
            0.0,
        );
        let direction_3d = Vec3::new(direction.x, direction.y, 0.0);
        let perpendicular = direction_3d.cross(Vec3::Z).normalize_or_zero();
        [
            Vec4::new(
                direction.x * self.direction_envelope.output,
                direction.y * self.direction_envelope.output,
                0.0,
                0.0,
            ),
            (vibration * self.vibration_envelope.output).extend(0.0),
            Vec4::new(0.1, 0.1, 0.1, 1.0),
            self.texture_phases,
            perpendicular.extend(0.0),
            direction_3d.extend(0.0),
            Vec3::Z.extend(0.0),
        ]
    }

    fn next_signed_random(&mut self) -> f32 {
        self.next_unit_random() * 2.0 - 1.0
    }

    fn next_unit_random(&mut self) -> f32 {
        self.random_state = self
            .random_state
            .wrapping_mul(214_013)
            .wrapping_add(2_531_011);
        let random_value = (self.random_state >> 16) & 0x7fff;
        random_value as f32 / 32_767.0
    }
}

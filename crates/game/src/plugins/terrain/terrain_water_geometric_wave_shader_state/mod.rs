//! Native four-channel geometric-water state written to `waterflat.fx`.

use bevy::prelude::*;
use openzt2_game_data::terrain::{
    TerrainWaterGeometricWavePresentation, TerrainWaterRippleWavePresentation,
};

use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;

const GEOMETRIC_AMPLITUDE_SCALE: f32 = 0.0019;
const RIPPLE_AMPLITUDE_SCALE: f32 = 0.0499;
const AMPLITUDE_BIAS: f32 = 0.0001;
const CHOP_SCALE: f32 = 0.5;
const RIPPLE_CHOP_BIAS: f32 = 0.001;
const MINIMUM_WAVELENGTH: f32 = 5.0;
const MAXIMUM_WAVELENGTH: f32 = 10.0;
const MINIMUM_CENTRE_X: f32 = -100.0;
const MAXIMUM_CENTRE_X: f32 = 0.0;
const MINIMUM_CENTRE_Y: f32 = -100.0;
const MAXIMUM_CENTRE_Y: f32 = 0.0;
const RANDOM_ATTACK_SECONDS: f32 = 1.0;
const RANDOM_RELEASE_SECONDS: f32 = 3.0;
const RANDOM_RAMP_OFFSET: f32 = 1_000_000.0;
const RANDOM_RAMP_DISTANCE: f32 = 100.0;
const PHASE_WAVELENGTH_SCALE: f32 = 188.495_56;
const RANDOM_RESPAWN_DELAY_SECONDS: f32 = 4.0;
const RIPPLE_STARTUP_DIVISOR: f32 = 3.0;
const INITIAL_WAVE_STEEPNESS: f32 = 20.0;
const WAVE_STEEPNESS_DENOMINATOR: f32 = 25.132_742;

#[derive(Clone, Debug)]
pub(super) struct NativeTerrainWaterGeometricWaveShaderState {
    channels: [NativeTerrainWaterGeometricWaveChannel; 4],
    geometric_minimum_amplitude_ratio: f32,
    geometric_maximum_amplitude_ratio: f32,
    ripple_configuration: NativeTerrainWaterImpactWaveConfiguration,
    pending_impact: Option<NativeTerrainWaterPendingImpactWave>,
    next_impact_channel_index: usize,
    random_state: u32,
}

#[derive(Clone, Copy, Debug)]
struct NativeTerrainWaterPendingImpactWave {
    centre: Vec2,
    strength: f32,
    channel_index: usize,
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct ActivateTerrainWaterImpactWave {
    pub surface: Entity,
    pub source_position: Vec2,
    pub native_strength: f32,
}

#[derive(Clone, Copy, Debug)]
struct NativeTerrainWaterImpactWaveConfiguration {
    minimum_amplitude_ratio: f32,
    maximum_amplitude_ratio: f32,
    chop: f32,
    lifespan_seconds: f32,
    activation_transition_seconds: f32,
    attack_seconds: f32,
    release_seconds: f32,
    speed_metres_per_second: f32,
    ramp_minimum_metres: f32,
    ramp_maximum_metres: f32,
}

#[derive(Clone, Copy, Debug)]
struct NativeTerrainWaterGeometricWaveChannel {
    amplitude: f32,
    wavelength: f32,
    centre_x: f32,
    centre_y: f32,
    lifespan_end_seconds: f32,
    attack_seconds: f32,
    release_seconds: f32,
    ramp_offset: f32,
    ramp_distance: f32,
    scheduled_spawn_seconds: f32,
    chop: f32,
    phase: f32,
    envelope: f32,
    elapsed_seconds: f32,
}

impl NativeTerrainWaterGeometricWaveShaderState {
    pub(super) fn from_authored_geometric_wave(
        geometric: TerrainWaterGeometricWavePresentation,
        ripple: TerrainWaterRippleWavePresentation,
        random_seed: u32,
    ) -> Self {
        let startup_divisor_seconds = ripple.startup_seconds / RIPPLE_STARTUP_DIVISOR;
        let mut state = Self {
            channels: [NativeTerrainWaterGeometricWaveChannel::cleared(); 4],
            geometric_minimum_amplitude_ratio: map_percent(
                geometric.minimum_amplitude_percent,
                GEOMETRIC_AMPLITUDE_SCALE,
                AMPLITUDE_BIAS,
            ),
            geometric_maximum_amplitude_ratio: map_percent(
                geometric.maximum_amplitude_percent,
                GEOMETRIC_AMPLITUDE_SCALE,
                AMPLITUDE_BIAS,
            ),
            ripple_configuration: NativeTerrainWaterImpactWaveConfiguration {
                minimum_amplitude_ratio: map_percent(
                    ripple.minimum_amplitude_percent,
                    RIPPLE_AMPLITUDE_SCALE,
                    AMPLITUDE_BIAS,
                ),
                maximum_amplitude_ratio: map_percent(
                    ripple.maximum_amplitude_percent,
                    RIPPLE_AMPLITUDE_SCALE,
                    AMPLITUDE_BIAS,
                ),
                chop: ripple.chop_percent / 100.0 * CHOP_SCALE + RIPPLE_CHOP_BIAS,
                lifespan_seconds: ripple.lifespan_seconds,
                activation_transition_seconds: startup_divisor_seconds * 2.0,
                attack_seconds: startup_divisor_seconds,
                release_seconds: (ripple.lifespan_seconds * 0.5)
                    .min(ripple.lifespan_seconds - startup_divisor_seconds),
                speed_metres_per_second: ripple.speed_metres_per_second,
                ramp_minimum_metres: ripple.ramp_minimum_metres,
                ramp_maximum_metres: ripple.ramp_maximum_metres,
            },
            pending_impact: None,
            next_impact_channel_index: 0,
            random_state: random_seed,
        };
        for channel_index in 0..state.channels.len() {
            state.spawn_random_wave_channel(channel_index, geometric.chop_percent);
        }
        state
    }

    fn native_msvcr80_random_unit_float(&mut self) -> f32 {
        self.random_state = self
            .random_state
            .wrapping_mul(214_013)
            .wrapping_add(2_531_011);
        ((self.random_state >> 16) & 0x7fff) as f32 / 32_767.0
    }

    fn spawn_random_wave_channel(&mut self, channel_index: usize, chop_percent: f32) {
        let wavelength = lerp(
            MINIMUM_WAVELENGTH,
            MAXIMUM_WAVELENGTH,
            self.native_msvcr80_random_unit_float(),
        );
        let amplitude_ratio = lerp(
            self.geometric_minimum_amplitude_ratio,
            self.geometric_maximum_amplitude_ratio,
            self.native_msvcr80_random_unit_float(),
        );
        let centre_x = lerp(
            MINIMUM_CENTRE_X,
            MAXIMUM_CENTRE_X,
            self.native_msvcr80_random_unit_float(),
        );
        let centre_y = lerp(
            MINIMUM_CENTRE_Y,
            MAXIMUM_CENTRE_Y,
            self.native_msvcr80_random_unit_float(),
        );
        self.channels[channel_index] = NativeTerrainWaterGeometricWaveChannel {
            amplitude: wavelength * amplitude_ratio,
            wavelength,
            centre_x,
            centre_y,
            lifespan_end_seconds: -1.0,
            attack_seconds: RANDOM_ATTACK_SECONDS,
            release_seconds: RANDOM_RELEASE_SECONDS,
            ramp_offset: RANDOM_RAMP_OFFSET,
            ramp_distance: RANDOM_RAMP_DISTANCE,
            scheduled_spawn_seconds: -1.0,
            chop: chop_percent / 100.0 * CHOP_SCALE,
            phase: 0.0,
            envelope: 0.0,
            elapsed_seconds: 0.0,
        };
    }

    pub(super) fn queue_native_impact_wave(&mut self, source_position: Vec2, native_strength: f32) {
        if self.channels.iter_mut().any(|channel| {
            let existing_centre = Vec2::new(channel.centre_x, channel.centre_y);
            if existing_centre.distance_squared(source_position) < 1.0 {
                channel.lifespan_end_seconds = channel.elapsed_seconds
                    + self.ripple_configuration.activation_transition_seconds;
                true
            } else {
                false
            }
        }) {
            return;
        }
        if self.pending_impact.is_none() {
            let channel_index = self.next_impact_channel_index;
            self.next_impact_channel_index = (channel_index + 1) % self.channels.len();
            let channel = &mut self.channels[channel_index];
            channel.lifespan_end_seconds = if channel.envelope < 0.05 {
                channel.elapsed_seconds + self.ripple_configuration.activation_transition_seconds
            } else {
                channel.elapsed_seconds
            };
            self.pending_impact = Some(NativeTerrainWaterPendingImpactWave {
                centre: source_position,
                strength: native_strength,
                channel_index,
            });
        }
    }

    fn spawn_pending_native_impact_wave(&mut self, impact: NativeTerrainWaterPendingImpactWave) {
        const IMPACT_MINIMUM_WAVELENGTH: f32 = 3.0;
        const IMPACT_MAXIMUM_WAVELENGTH: f32 = 5.0;
        const IMPACT_RAMP_OFFSET_SCALE: f32 = 0.25;
        const IMPACT_RAMP_DISTANCE_SCALE: f32 = 0.5;

        let configuration = self.ripple_configuration;
        let wavelength = lerp(
            IMPACT_MINIMUM_WAVELENGTH,
            IMPACT_MAXIMUM_WAVELENGTH,
            impact.strength,
        );
        let amplitude_ratio = lerp(
            configuration.minimum_amplitude_ratio,
            configuration.maximum_amplitude_ratio,
            impact.strength,
        );
        self.channels[impact.channel_index] = NativeTerrainWaterGeometricWaveChannel {
            amplitude: wavelength * amplitude_ratio,
            wavelength,
            centre_x: impact.centre.x,
            centre_y: impact.centre.y,
            lifespan_end_seconds: configuration.lifespan_seconds,
            attack_seconds: configuration.attack_seconds,
            release_seconds: configuration.release_seconds,
            ramp_offset: wavelength * IMPACT_RAMP_OFFSET_SCALE,
            ramp_distance: wavelength * IMPACT_RAMP_DISTANCE_SCALE,
            scheduled_spawn_seconds: lerp(
                configuration.ramp_minimum_metres,
                configuration.ramp_maximum_metres,
                impact.strength,
            ),
            chop: configuration.chop,
            phase: 0.0,
            envelope: 0.0,
            elapsed_seconds: 0.0,
        };
    }

    pub(super) fn advance_wave_channels(&mut self, elapsed_seconds: f32) {
        let mut pending_spawn_exists = false;
        for channel_index in 0..self.channels.len() {
            let channel_before_update = self.channels[channel_index];
            if channel_before_update.lifespan_end_seconds > 0.0 {
                pending_spawn_exists = true;
                if channel_before_update.lifespan_end_seconds
                    < channel_before_update.elapsed_seconds
                {
                    if self
                        .pending_impact
                        .is_some_and(|impact| impact.channel_index == channel_index)
                    {
                        let impact = self.pending_impact.take().expect("impact was just checked");
                        self.spawn_pending_native_impact_wave(impact);
                    } else {
                        self.spawn_random_wave_channel(
                            channel_index,
                            channel_before_update.chop / CHOP_SCALE * 100.0,
                        );
                    }
                }
            }
            advance_channel(
                &mut self.channels[channel_index],
                elapsed_seconds,
                self.ripple_configuration.speed_metres_per_second,
            );
        }
        if !pending_spawn_exists {
            let oldest_channel_index = self
                .channels
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    left.elapsed_seconds.total_cmp(&right.elapsed_seconds)
                })
                .map_or(0, |(index, _)| index);
            let channel = &mut self.channels[oldest_channel_index];
            channel.lifespan_end_seconds = channel.elapsed_seconds + RANDOM_RESPAWN_DELAY_SECONDS;
        }
    }

    pub(super) fn bind_wave_channels_to_effect_pass(
        &self,
        water_height: f32,
        material: &mut EffectPassMaterial,
    ) {
        material.bind_float_vector_effect_semantic(
            "Frequency",
            Vec4::from_array(
                self.channels
                    .map(|channel| std::f32::consts::TAU / channel.wavelength),
            ),
        );
        material.bind_float_vector_effect_semantic(
            "Phase",
            Vec4::from_array(self.channels.map(|channel| channel.phase)),
        );
        material.bind_float_vector_effect_semantic(
            "Amplitude",
            Vec4::from_array(
                self.channels
                    .map(|channel| channel.amplitude * channel.envelope),
            ),
        );
        material.bind_float_vector_effect_semantic(
            "PosX",
            Vec4::from_array(self.channels.map(|channel| channel.centre_x)),
        );
        material.bind_float_vector_effect_semantic(
            "PosY",
            Vec4::from_array(self.channels.map(|channel| channel.centre_y)),
        );
        material.bind_float_vector_effect_semantic(
            "K",
            Vec4::from_array(self.channels.map(|channel| channel.native_wave_steepness())),
        );
        material.bind_float_vector_effect_semantic(
            "DepthOffset",
            Vec4::new(
                water_height,
                water_height,
                water_height + 0.001,
                water_height,
            ),
        );
        material.bind_float_vector_effect_semantic("DepthScale", Vec4::new(2.0, 2.0, 2.0, 1.0));
        material.bind_float_vector_effect_semantic(
            "RampOffset",
            Vec4::from_array(self.channels.map(|channel| channel.ramp_offset)),
        );
        material.bind_float_vector_effect_semantic(
            "RampScale",
            Vec4::from_array(self.channels.map(|channel| channel.ramp_distance.recip())),
        );
    }

    /// Bounds the shipped watersurface.h displacement in source XY and Z.
    /// Its wrapped seventh/sixth-order sine/cosine polynomials stay below
    /// 1.01/1.22 in magnitude; ramp attenuation is in [0, 1]. Horizontal
    /// displacement additionally multiplies the original input vertex height.
    pub(super) fn maximum_horizontal_and_vertical_shader_displacement(
        &self,
        maximum_absolute_input_height: f32,
    ) -> Option<Vec2> {
        let mut maximum_displacement = Vec2::ZERO;
        for channel in self.channels {
            let amplitude = channel.amplitude * channel.envelope;
            maximum_displacement += Vec2::new(
                (amplitude * std::f32::consts::TAU / channel.wavelength
                    * channel.native_wave_steepness())
                .abs(),
                amplitude.abs(),
            );
        }
        maximum_displacement *= Vec2::new(1.22 * maximum_absolute_input_height, 1.01);
        maximum_displacement += Vec2::splat(0.01);
        maximum_displacement
            .is_finite()
            .then_some(maximum_displacement)
    }
}

pub(super) fn apply_authored_water_impact_events_to_geometric_wave_channels(
    mut impacts: MessageReader<ActivateTerrainWaterImpactWave>,
    water_surfaces: Query<(
        &AuthoredEffectTechniquePassSubmissionOrder,
        &super::terrain_water_renderer_types::TerrainWaterWaveStateKey,
    )>,
    mut targets: Option<
        ResMut<super::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets>,
    >,
) {
    for impact in impacts.read() {
        if let Some((_, key)) = water_surfaces
            .iter()
            .find(|(order, _)| order.belongs_to_technique_invocation(impact.surface))
        {
            if let Some(targets) = targets.as_deref_mut() {
                targets.queue_wave_impact(*key, impact.source_position, impact.native_strength);
            }
        }
    }
}

impl NativeTerrainWaterGeometricWaveChannel {
    fn native_wave_steepness(self) -> f32 {
        let amplitude_ratio = self.amplitude / self.wavelength;
        if self.chop / (INITIAL_WAVE_STEEPNESS * WAVE_STEEPNESS_DENOMINATOR) < amplitude_ratio {
            self.chop / (amplitude_ratio * WAVE_STEEPNESS_DENOMINATOR)
        } else {
            INITIAL_WAVE_STEEPNESS
        }
    }

    const fn cleared() -> Self {
        Self {
            amplitude: 0.0,
            wavelength: 0.0,
            centre_x: 0.0,
            centre_y: 0.0,
            lifespan_end_seconds: -1.0,
            attack_seconds: 0.0,
            release_seconds: 0.0,
            ramp_offset: 0.0,
            ramp_distance: 0.0,
            scheduled_spawn_seconds: -1.0,
            chop: 0.0,
            phase: 0.0,
            envelope: 0.0,
            elapsed_seconds: 0.0,
        }
    }
}

fn advance_channel(
    channel: &mut NativeTerrainWaterGeometricWaveChannel,
    elapsed_seconds: f32,
    ripple_speed_metres_per_second: f32,
) {
    channel.elapsed_seconds += elapsed_seconds;
    let remaining_seconds = channel.lifespan_end_seconds - channel.elapsed_seconds;
    channel.envelope = if channel.elapsed_seconds < channel.attack_seconds {
        channel.elapsed_seconds / channel.attack_seconds
    } else if channel.lifespan_end_seconds > 0.0 && remaining_seconds < channel.release_seconds {
        (remaining_seconds / channel.release_seconds).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let inverse_wave_distance_scale = (channel.wavelength / PHASE_WAVELENGTH_SCALE).sqrt();
    channel.phase += elapsed_seconds / inverse_wave_distance_scale;
    if channel.scheduled_spawn_seconds > channel.ramp_offset {
        channel.ramp_offset +=
            ripple_speed_metres_per_second * elapsed_seconds / inverse_wave_distance_scale;
        channel.ramp_distance = channel.ramp_distance.max(channel.ramp_offset);
    }
}

fn map_percent(percent: f32, scale: f32, bias: f32) -> f32 {
    percent / 100.0 * scale + bias
}

fn lerp(minimum: f32, maximum: f32, fraction: f32) -> f32 {
    (maximum - minimum) * fraction + minimum
}

#[cfg(test)]
mod geometric_wave_state_tests {
    use super::*;

    #[test]
    fn native_random_wave_defaults_keep_phase_finite_through_respawn() {
        for seed in 0..128 {
            let mut state =
                NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
                    TerrainWaterGeometricWavePresentation {
                        minimum_amplitude_percent: 10.0,
                        maximum_amplitude_percent: 20.0,
                        chop_percent: 25.0,
                    },
                    TerrainWaterRippleWavePresentation {
                        lifespan_seconds: 4.0,
                        startup_seconds: 1.0,
                        minimum_amplitude_percent: 10.0,
                        maximum_amplitude_percent: 20.0,
                        chop_percent: 25.0,
                        speed_metres_per_second: 1.0,
                        ramp_minimum_metres: 1.0,
                        ramp_maximum_metres: 5.0,
                    },
                    seed,
                );
            for _ in 0..120 {
                state.advance_wave_channels(0.1);
                for channel in state.channels {
                    assert!((5.0..=10.0).contains(&channel.wavelength));
                    assert!((-100.0..=0.0).contains(&channel.centre_x));
                    assert!((-100.0..=0.0).contains(&channel.centre_y));
                    assert!(channel.phase.is_finite());
                }
                assert!(state
                    .maximum_horizontal_and_vertical_shader_displacement(50.0)
                    .is_some());
            }
        }
    }

    #[test]
    fn native_steepness_uses_chop_instead_of_the_fade_envelope() {
        let mut channel = NativeTerrainWaterGeometricWaveChannel::cleared();
        channel.amplitude = 0.1;
        channel.wavelength = 5.0;
        channel.chop = 0.125;
        channel.envelope = 0.0;
        let expected = 0.125 / (0.02 * WAVE_STEEPNESS_DENOMINATOR);
        assert!((channel.native_wave_steepness() - expected).abs() < 0.000_001);
        channel.envelope = 1.0;
        assert!((channel.native_wave_steepness() - expected).abs() < 0.000_001);
    }
}

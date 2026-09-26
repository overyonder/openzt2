use bevy::prelude::*;
use openzt2_game_data::{world_definitions::environment::WeatherDefinition, AssetId};

use crate::assets::effect::SpawnParticleEffectEmitter;
use crate::assets::effect::StopParticleEffectsAttachedToEntity;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::audio::audio_playback_message_types::PlayAudioCue;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

use super::{
    daylight_curve_sampling::{
        calculate_daylight_fraction, daylight_fraction_unit_is_within_inclusive_window,
        encode_daylight_fraction_as_u16,
    },
    environment_random_sampling::{
        random_probability_threshold_passes, sample_inclusive_u32_range,
    },
    environment_state_types::{EnvironmentRandom, Wind, WorldEnvironment},
    weather_rules::{
        generate_deterministic_wind_from_speed_range,
        weather_transition_has_reached_completion_tick,
    },
    weather_types::{
        PendingWeatherOperation, SetWeather, Weather, WeatherChanged, WeatherRequestApplied,
        WeatherRequestRejected, WeatherTransition,
    },
};

pub(super) fn apply_requested_weather_transitions(
    mut commands: Commands,
    mut requests: MessageReader<SetWeather>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    mut environments: Query<(
        Entity,
        &WorldEnvironment,
        &mut Weather,
        &mut Wind,
        &mut EnvironmentRandom,
        Option<&WeatherTransition>,
        Option<&PendingWeatherOperation>,
    )>,
    mut applied_results: MessageWriter<WeatherRequestApplied>,
    mut rejected_results: MessageWriter<WeatherRequestRejected>,
    mut weather_changes: MessageWriter<WeatherChanged>,
    mut audio: MessageWriter<PlayAudioCue>,
    mut effects: MessageWriter<SpawnParticleEffectEmitter>,
    mut stop_effects: MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    if requests.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut accepted_request_during_current_tick = false;
    for request in requests.read() {
        let Some((
            environment_entity,
            environment,
            mut weather,
            mut wind,
            mut random,
            transition,
            pending_operation,
        )) = environments.iter_mut().find(|(_, environment, ..)| {
            definitions
                .find_weather(request.definition)
                .is_some_and(|definition| definition.environment.0 == environment.definition.0)
        })
        else {
            reject_weather_request(request, &mut rejected_results);
            continue;
        };
        let Some(weather_definition) = definitions.find_weather(request.definition) else {
            reject_weather_request(request, &mut rejected_results);
            continue;
        };
        let Some(environment_definition) = definitions.find_environment(environment.definition)
        else {
            reject_weather_request(request, &mut rejected_results);
            continue;
        };
        let daylight_fraction_unit = encode_daylight_fraction_as_u16(
            calculate_daylight_fraction(clock.tick_in_day, definitions.timing().ticks_per_day)
                .unwrap_or(0.0),
        );
        let Some(authored_transition_rule) =
            environment_definition.transitions.iter().find(|rule| {
                rule.from.0 == weather.definition.0
                    && rule.to.0 == request.definition.0
                    && daylight_fraction_unit_is_within_inclusive_window(
                        daylight_fraction_unit,
                        rule.day_fraction,
                    )
            })
        else {
            reject_weather_request(request, &mut rejected_results);
            continue;
        };
        if accepted_request_during_current_tick
            || transition.is_some()
            || pending_operation.is_some()
            || weather.definition == request.definition
        {
            reject_weather_request(request, &mut rejected_results);
            continue;
        }

        let previous_weather = weather.definition;
        let transition_ticks = request
            .transition_ticks
            .unwrap_or(authored_transition_rule.transition_ticks);
        if transition_ticks == 0 {
            set_current_weather_and_regenerate_wind(
                &mut weather,
                &mut wind,
                &mut random.0,
                request.definition,
                weather_definition,
            );
            applied_results.write(WeatherRequestApplied {
                operation: request.operation,
                definition: request.definition,
            });
            weather_changes.write(WeatherChanged {
                previous: previous_weather,
                current: request.definition,
            });
            project_weather_audio_and_effects(
                environment_entity,
                weather_definition,
                definitions,
                clock.tick,
                &mut audio,
                &mut effects,
                &mut stop_effects,
            );
        } else {
            commands.entity(environment_entity).insert((
                WeatherTransition {
                    from: previous_weather,
                    to: request.definition,
                    start_tick: clock.tick,
                    duration_ticks: transition_ticks,
                },
                PendingWeatherOperation {
                    operation: request.operation,
                    definition: request.definition,
                },
            ));
        }
        accepted_request_during_current_tick = true;
    }
}

pub(super) fn advance_automatic_and_requested_weather_transitions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut previously_advanced_tick: Local<Option<u64>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut environments: Query<(
        Entity,
        &WorldEnvironment,
        &mut Weather,
        Option<&WeatherTransition>,
        Option<&PendingWeatherOperation>,
        &mut Wind,
        &mut EnvironmentRandom,
    )>,
    mut applied_results: MessageWriter<WeatherRequestApplied>,
    mut weather_changes: MessageWriter<WeatherChanged>,
    mut audio: MessageWriter<PlayAudioCue>,
    mut effects: MessageWriter<SpawnParticleEffectEmitter>,
    mut stop_effects: MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    if previously_advanced_tick.is_some_and(|tick| tick == clock.tick) {
        return;
    }
    *previously_advanced_tick = Some(clock.tick);
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };

    for (
        environment_entity,
        environment,
        mut weather,
        transition,
        pending_operation,
        mut wind,
        mut random,
    ) in &mut environments
    {
        if let Some(transition) = transition {
            if !weather_transition_has_reached_completion_tick(clock.tick, *transition) {
                continue;
            }
            let Some(weather_definition) = definitions.find_weather(transition.to) else {
                continue;
            };
            let previous_weather = weather.definition;
            set_current_weather_and_regenerate_wind(
                &mut weather,
                &mut wind,
                &mut random.0,
                transition.to,
                weather_definition,
            );
            commands
                .entity(environment_entity)
                .remove::<WeatherTransition>();
            if let Some(pending_operation) = pending_operation {
                applied_results.write(WeatherRequestApplied {
                    operation: pending_operation.operation,
                    definition: pending_operation.definition,
                });
                commands
                    .entity(environment_entity)
                    .remove::<PendingWeatherOperation>();
            }
            weather_changes.write(WeatherChanged {
                previous: previous_weather,
                current: weather.definition,
            });
            project_weather_audio_and_effects(
                environment_entity,
                weather_definition,
                definitions,
                clock.tick,
                &mut audio,
                &mut effects,
                &mut stop_effects,
            );
            continue;
        }

        weather.elapsed_ticks = weather.elapsed_ticks.saturating_add(1);
        if weather.elapsed_ticks < weather.duration_ticks {
            continue;
        }
        let Some(environment_definition) = definitions.find_environment(environment.definition)
        else {
            continue;
        };
        let daylight_fraction_unit = encode_daylight_fraction_as_u16(
            calculate_daylight_fraction(clock.tick_in_day, definitions.timing().ticks_per_day)
                .unwrap_or(0.0),
        );
        let selected_transition = environment_definition.transitions.iter().find(|rule| {
            rule.from.0 == weather.definition.0
                && daylight_fraction_unit_is_within_inclusive_window(
                    daylight_fraction_unit,
                    rule.day_fraction,
                )
                && random_probability_threshold_passes(rule.probability, &mut random.0)
        });
        let Some(selected_transition) = selected_transition else {
            weather.elapsed_ticks = 0;
            continue;
        };
        let target_weather_identifier = selected_transition.to;
        if selected_transition.transition_ticks == 0 {
            let Some(weather_definition) = definitions.find_weather(target_weather_identifier)
            else {
                continue;
            };
            let previous_weather = weather.definition;
            set_current_weather_and_regenerate_wind(
                &mut weather,
                &mut wind,
                &mut random.0,
                target_weather_identifier,
                weather_definition,
            );
            weather_changes.write(WeatherChanged {
                previous: previous_weather,
                current: target_weather_identifier,
            });
            project_weather_audio_and_effects(
                environment_entity,
                weather_definition,
                definitions,
                clock.tick,
                &mut audio,
                &mut effects,
                &mut stop_effects,
            );
        } else {
            commands
                .entity(environment_entity)
                .insert(WeatherTransition {
                    from: weather.definition,
                    to: target_weather_identifier,
                    start_tick: clock.tick,
                    duration_ticks: selected_transition.transition_ticks,
                });
        }
    }
}

pub(super) fn sample_authored_weather_duration_ticks(
    definition: &WeatherDefinition,
    random: &mut DeterministicRng,
) -> u64 {
    u64::from(sample_inclusive_u32_range(
        definition.duration_ticks,
        random,
    ))
}

pub(super) fn project_weather_audio_and_effects(
    environment_entity: Entity,
    definition: &WeatherDefinition,
    definitions: WorldDefinitionsView<'_>,
    selection: u64,
    audio: &mut MessageWriter<PlayAudioCue>,
    effects: &mut MessageWriter<SpawnParticleEffectEmitter>,
    stop_effects: &mut MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    let audio_cue_identifier = definition.audio;
    if audio_cue_identifier != AssetId::default() {
        audio.write(PlayAudioCue {
            cue: audio_cue_identifier,
            emitter: Some(environment_entity),
            selection,
            priority: 0,
            force_looped: false,
        });
    }
    stop_effects.write(StopParticleEffectsAttachedToEntity {
        attached_entity: environment_entity,
    });
    let effect_asset_identifier = definition.effect_asset;
    let effect_emitter_identifier = definition.effect_emitter;
    if let Some(effect) = definitions.effect(effect_asset_identifier) {
        effects.write(SpawnParticleEffectEmitter {
            particle_effect_asset: effect,
            emitter_id: effect_emitter_identifier,
            parent_entity: Some(environment_entity),
            effect_transform: Transform::IDENTITY,
            manual_particle_count: 0,
        });
    }
}

fn reject_weather_request(
    request: &SetWeather,
    rejected_results: &mut MessageWriter<WeatherRequestRejected>,
) {
    rejected_results.write(WeatherRequestRejected {
        operation: request.operation,
        definition: request.definition,
    });
}

fn set_current_weather_and_regenerate_wind(
    weather: &mut Weather,
    wind: &mut Wind,
    random: &mut DeterministicRng,
    weather_identifier: AssetId,
    definition: &WeatherDefinition,
) {
    weather.definition = weather_identifier;
    weather.elapsed_ticks = 0;
    weather.duration_ticks = sample_authored_weather_duration_ticks(definition, random);
    *wind = generate_deterministic_wind_from_speed_range(definition.wind_mps, random);
}

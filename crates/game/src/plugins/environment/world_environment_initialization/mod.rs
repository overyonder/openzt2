use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::effect::SpawnParticleEffectEmitter;
use crate::assets::effect::StopParticleEffectsAttachedToEntity;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::plugins::audio::audio_environment_types::AudioSoundscape;
use crate::plugins::audio::audio_playback_message_types::PlayAudioCue;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    ambient_animal_types::AmbientAnimalSpawner,
    daylight_curve_sampling::calculate_daylight_fraction,
    environment_presentation_types::EnvironmentPresentationPending,
    environment_random_sampling::sample_inclusive_u32_range,
    environment_state_types::{
        AuthoredWindShaderPresentationState, Daylight, EnvironmentRandom, WorldEnvironment,
    },
    weather_rules::generate_deterministic_wind_from_speed_range,
    weather_transition_lifecycle::{
        project_weather_audio_and_effects, sample_authored_weather_duration_ticks,
    },
    weather_types::Weather,
};

#[derive(Component)]
pub(super) struct WorldEnvironmentInitialized;

pub(super) fn initialize_world_environment_from_loaded_scenario_and_definitions(
    mut commands: Commands,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    clock: Option<Res<ZooClock>>,
    seed: Option<Res<ZooSeed>>,
    allocator: Option<ResMut<PersistentIdAllocator>>,
    roots: Query<
        (Entity, &WorldRoot),
        (
            With<WorldLoadCompleted>,
            Without<WorldEnvironmentInitialized>,
        ),
    >,
    mut audio: MessageWriter<PlayAudioCue>,
    mut effects: MessageWriter<SpawnParticleEffectEmitter>,
    mut stop_effects: MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    let (Some(clock), Some(seed), Some(mut allocator)) = (clock, seed, allocator) else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(scenarios) = active_scenarios.get(&scenarios) else {
        return;
    };
    for (root, world) in &roots {
        let Some((environment_id, biome_id)) =
            select_scenario_environment_and_biome(scenarios, world.scenario)
        else {
            warn!(scenario = ?world.scenario, "loaded map has no native environment binding");
            continue;
        };
        let Some(definition) = definitions.find_environment(environment_id) else {
            warn!(
                ?environment_id,
                "native environment binding has no definition"
            );
            continue;
        };
        let Ok(persistent_id) = allocator.allocate(root) else {
            continue;
        };
        let mut random = DeterministicRng::from_entity(*seed, persistent_id, RngDomain::Weather);
        let initial_weather_id = definition.initial_weather;
        let initial_weather = (initial_weather_id != AssetId::default())
            .then(|| definitions.find_weather(initial_weather_id))
            .flatten();
        if initial_weather_id != AssetId::default() && initial_weather.is_none() {
            warn!(environment = ?environment_id, weather = ?initial_weather_id, "native environment has no initial weather definition");
            continue;
        }
        let ticks_per_day = definitions.timing().ticks_per_day;
        let Some(fraction) = calculate_daylight_fraction(clock.tick_in_day, ticks_per_day) else {
            continue;
        };
        let wind_range = initial_weather.map_or(
            [definition.wind_mps[0], definition.wind_mps[1]],
            |weather| [weather.wind_mps[0], weather.wind_mps[1]],
        );
        let wind = generate_deterministic_wind_from_speed_range(wind_range, &mut random);
        let wind_shader_presentation =
            AuthoredWindShaderPresentationState::from_wind(wind, random.next_u32());
        let weather = initial_weather.map(|initial_weather| Weather {
            definition: initial_weather.id,
            elapsed_ticks: 0,
            duration_ticks: sample_authored_weather_duration_ticks(initial_weather, &mut random),
        });
        let mut environment_commands = commands.spawn((
            WorldEnvironment {
                definition: environment_id,
            },
            Daylight { fraction },
            EnvironmentPresentationPending,
            AudioSoundscape {
                biome: biome_id,
                selection: (u64::from(random.next_u32()) << 32) | u64::from(random.next_u32()),
                day_mask: 0b11,
            },
            wind,
            wind_shader_presentation,
            EnvironmentRandom(random),
            Transform::IDENTITY,
            WorldMember { root },
            persistent_id,
        ));
        if let Some(weather) = weather {
            environment_commands.insert(weather);
        }
        let environment_entity = environment_commands.id();
        commands.entity(root).insert(WorldEnvironmentInitialized);
        if let Some(initial_weather) = initial_weather {
            project_weather_audio_and_effects(
                environment_entity,
                initial_weather,
                definitions,
                clock.tick,
                &mut audio,
                &mut effects,
                &mut stop_effects,
            );
        }

        for ambient in definition
            .ambient
            .iter()
            .filter_map(|identifier| definitions.find_ambient_spawn(*identifier))
        {
            let Ok(id) = allocator.allocate(root) else {
                break;
            };
            let mut random = DeterministicRng::from_entity(*seed, id, RngDomain::AmbientSpawner);
            let interval = sample_inclusive_u32_range(
                [
                    ambient.spawn_interval_ticks[0],
                    ambient.spawn_interval_ticks[1],
                ],
                &mut random,
            );
            commands.spawn((
                AmbientAnimalSpawner {
                    definition: ambient.id,
                    next_tick: clock.tick.saturating_add(u64::from(interval)),
                    random,
                },
                WorldMember { root },
                id,
            ));
        }
    }
}

fn select_scenario_environment_and_biome(
    scenarios: crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenariosView<'_>,
    scenario_identifier: AssetId,
) -> Option<(AssetId, AssetId)> {
    let map_identifier = scenarios
        .campaign_scenario(scenario_identifier)
        .map(|scenario| scenario.map)
        .unwrap_or(scenario_identifier);
    scenarios
        .map(map_identifier)
        .map(|map| (map.environment, map.biome))
}

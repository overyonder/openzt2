use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::environment::{AmbientClass, AmbientSpawnDefinition},
    AssetId,
};

use crate::assets::effect::SpawnParticleEffectEmitter;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::audio::audio_environment_types::AmbientAudioEmitter;
use crate::plugins::locomotion::locomotion_types::LocomotionMode;
use crate::plugins::locomotion::locomotion_types::SpatialGrid;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::terrain::terrain_world_sampling::sample_authored_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabPresentation;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    ambient_animal_spawn_rules::ambient_animal_population_is_below_authored_maximum,
    ambient_animal_types::{AmbientAnimal, AmbientAnimalSpawner},
    daylight_curve_sampling::{
        calculate_daylight_fraction, daylight_fraction_unit_is_within_inclusive_window,
        encode_daylight_fraction_as_u16,
    },
    environment_random_sampling::{
        random_probability_threshold_passes, sample_inclusive_u32_range,
    },
};

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_eligible_ambient_animals_from_authored_environment_spawners(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_index: Res<TerrainIndex>,
    spatial_grid: Res<SpatialGrid>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut spawners: Query<(&WorldMember, &mut AmbientAnimalSpawner)>,
    ambient_animals: Query<(&AmbientAnimal, &WorldMember)>,
    mut effects: MessageWriter<SpawnParticleEffectEmitter>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (world_member, mut spawner) in &mut spawners {
        if clock.tick < spawner.next_tick {
            continue;
        }
        let Some(definition) = definitions.find_ambient_spawn(spawner.definition) else {
            continue;
        };
        let spawn_interval_ticks =
            sample_inclusive_u32_range(definition.spawn_interval_ticks, &mut spawner.random);
        spawner.next_tick = clock.tick.saturating_add(u64::from(spawn_interval_ticks));

        let current_population = ambient_animals
            .iter()
            .filter(|(animal, animal_world_member)| {
                animal_world_member.root == world_member.root
                    && animal.definition == spawner.definition
            })
            .count();
        if !ambient_animal_population_is_below_authored_maximum(
            current_population,
            definition.population,
        ) || !random_probability_threshold_passes(definition.probability, &mut spawner.random)
        {
            continue;
        }

        let daylight_fraction_unit = encode_daylight_fraction_as_u16(
            calculate_daylight_fraction(clock.tick_in_day, definitions.timing().ticks_per_day)
                .unwrap_or(0.0),
        );
        if !daylight_fraction_unit_is_within_inclusive_window(
            daylight_fraction_unit,
            definition.day_fraction,
        ) {
            continue;
        }
        let Some(position) = select_ambient_animal_position_on_eligible_terrain(
            definition,
            &terrain_assets,
            &terrain_index,
            &spatial_grid,
            &terrain_chunks,
            &mut spawner.random,
        ) else {
            continue;
        };
        let Some(prefab_index) = spawner.random.range_u32(definition.prefabs.len() as u32) else {
            continue;
        };
        let prefab_identifier = definition.prefabs[prefab_index as usize];
        let Some(prefab_handle) = definitions.scene(prefab_identifier) else {
            continue;
        };
        let lifetime_ticks =
            sample_inclusive_u32_range(definition.lifetime_ticks, &mut spawner.random);
        let locomotion_mode = match &definition.class {
            AmbientClass::Air => LocomotionMode::Flight,
            AmbientClass::Ground => LocomotionMode::Ground,
            AmbientClass::Water => LocomotionMode::Swim,
        };
        let heading_radians = spawner.random.unit_f32() * std::f32::consts::TAU;
        let ambient_animal = commands
            .spawn((
                AmbientAnimal {
                    definition: spawner.definition,
                    despawn_tick: clock.tick.saturating_add(u64::from(lifetime_ticks)),
                },
                PrefabPresentation::new(prefab_handle),
                locomotion_mode,
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(heading_radians)),
                Visibility::Inherited,
                *world_member,
            ))
            .id();

        let audio_identifier = definition.audio;
        if audio_identifier != AssetId::default() {
            commands.entity(ambient_animal).insert(AmbientAudioEmitter {
                definition: spawner.definition,
                cue: audio_identifier,
                selection: (u64::from(spawner.random.next_u32()) << 32)
                    | u64::from(spawner.random.next_u32()),
            });
        }
        let effect_asset_identifier = definition.effect_asset;
        let effect_emitter_identifier = definition.effect_emitter;
        if let Some(effect) = definitions.effect(effect_asset_identifier) {
            effects.write(SpawnParticleEffectEmitter {
                particle_effect_asset: effect,
                emitter_id: effect_emitter_identifier,
                parent_entity: Some(ambient_animal),
                effect_transform: Transform::IDENTITY,
                manual_particle_count: 0,
            });
        }
    }
}

pub(super) fn despawn_ambient_animals_after_authored_lifetime_expires(
    mut commands: Commands,
    clock: Res<ZooClock>,
    ambient_animals: Query<(Entity, &AmbientAnimal)>,
) {
    for (entity, ambient_animal) in &ambient_animals {
        if clock.tick >= ambient_animal.despawn_tick {
            commands.entity(entity).despawn();
        }
    }
}

fn select_ambient_animal_position_on_eligible_terrain(
    definition: &AmbientSpawnDefinition,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_index: &TerrainIndex,
    spatial_grid: &SpatialGrid,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    random: &mut DeterministicRng,
) -> Option<Vec3> {
    if spatial_grid.width == 0 || spatial_grid.height == 0 {
        return None;
    }
    let cell_index = random.range_u32(spatial_grid.width.checked_mul(spatial_grid.height)?)?;
    let cell_coordinates = spatial_grid.cell_xy(cell_index)?;
    let position_jitter =
        Vec2::new(random.unit_f32(), random.unit_f32()) * spatial_grid.cell_size_m;
    let world_horizontal_position = spatial_grid.origin
        + cell_coordinates.as_vec2() * spatial_grid.cell_size_m
        + position_jitter;
    let chunk_entity = terrain_chunk_at(terrain_index, world_horizontal_position)?;
    let (terrain_chunk, edited_samples) = terrain_chunks.get(chunk_entity).ok()?;
    let terrain = terrain_assets.get(&terrain_chunk.asset)?;
    let terrain_sample = sample_authored_terrain(
        terrain_chunk,
        terrain,
        edited_samples,
        world_horizontal_position,
    )?;
    let biome_identifier = terrain_sample.biome?.id;
    if !definition.biomes.contains(&biome_identifier) {
        return None;
    }
    match &definition.class {
        AmbientClass::Air => Some(Vec3::new(
            world_horizontal_position.x,
            terrain_sample.geometry.height_m,
            world_horizontal_position.y,
        )),
        AmbientClass::Ground if terrain_sample.geometry.water_height_m.is_none() => {
            Some(Vec3::new(
                world_horizontal_position.x,
                terrain_sample.geometry.height_m,
                world_horizontal_position.y,
            ))
        }
        AmbientClass::Water => terrain_sample.geometry.water_height_m.map(|water_height| {
            Vec3::new(
                world_horizontal_position.x,
                water_height,
                world_horizontal_position.y,
            )
        }),
        _ => None,
    }
}

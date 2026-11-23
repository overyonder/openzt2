use std::time::Duration;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::{
    audio::audio_asset_types::{AudioAsset, AudioAssets, AudioView},
    effect::{SpawnParticleEffectEmitter, StopParticleEffectsAttachedToEntity},
    graphics_settings_types::GraphicsSettings,
    scene_prefab::ScenePrefabAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    world_definitions::environment::{AmbientClass, AmbientSpawnDefinition},
    AssetId,
};
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::DeterministicRng, simulation_clock_types::ZooClock,
    },
    world_spawn::{
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember},
        world_terrain_hydration::WorldTerrainHorizontalBounds,
    },
};
use openzt2_zoo_state::{
    audio::audio_environment_types::AmbientAudioEmitter,
    environment::environment_state_types::{WorldEnvironment, ZooTickAdvanced},
    locomotion::locomotion_types::LocomotionMode,
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_authored_terrain, terrain_chunk_at},
    },
};

use super::{
    ambient_animal_types::{
        AmbientAnimal, AmbientAnimalOfWorld, AmbientSpawnCadence, WorldAmbientAnimals,
    },
    daylight_curve_sampling::{
        daylight_fraction_unit_is_within_inclusive_window, encode_daylight_fraction_as_u16,
    },
    environment_presentation_types::WorldEnvironments,
    environment_random_sampling::{
        random_probability_threshold_passes, sample_inclusive_u32_range,
    },
    environment_state_types::Daylight,
};

const AMBIENT_PLACEMENT_TRIES: usize = 10;

struct AmbientSpawn<'a> {
    definition: AssetId,
    object: Option<AssetId>,
    prefab: Handle<ScenePrefabAsset>,
    class: AmbientClass,
    local_rules: Option<&'a AmbientSpawnDefinition>,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn attempt_world_ambient_spawn(
    mut commands: Commands,
    mut ticks: MessageReader<ZooTickAdvanced>,
    timing: Res<Time<Fixed>>,
    graphics: Option<Res<GraphicsSettings>>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_index: Res<AudioAssets>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    environments: Query<(&WorldEnvironment, &Daylight)>,
    living: Query<&AmbientAnimal>,
    mut worlds: Query<(
        Entity,
        &ZooClock,
        &TerrainIndex,
        &WorldTerrainHorizontalBounds,
        &WorldEnvironments,
        Option<&WorldAmbientAnimals>,
        &mut AmbientSpawnCadence,
    )>,
    mut effects: MessageWriter<SpawnParticleEffectEmitter>,
) {
    let Some(&ZooTickAdvanced(tick)) = ticks.read().last() else {
        return;
    };
    if graphics.is_some_and(|graphics| !graphics.ambient_effects) {
        return;
    }
    let Some(catalogue) = definitions.get(&definition_assets) else {
        return;
    };
    let Some(audio) = audio_index.get(&audio_assets) else {
        return;
    };
    let Some(policy) = audio.ambient() else {
        return;
    };
    for (root, clock, terrain, bounds, owned_environments, members, mut cadence) in &mut worlds {
        if clock.tick != tick || policy.voice_limit == 0 {
            continue;
        }
        let Some((environment, daylight)) = owned_environments
            .iter()
            .find_map(|entity| environments.get(entity).ok())
        else {
            continue;
        };
        let Some(definition) = catalogue.find_environment(environment.definition) else {
            continue;
        };
        if !cadence.timer.tick(timing.timestep()).is_finished() {
            continue;
        }
        let admitted = cadence.random.unit_f32() <= policy.probability
            && members.map_or(0, RelationshipTarget::len) < usize::from(policy.voice_limit);
        let attempt = if admitted {
            let phase = encode_daylight_fraction_as_u16(daylight.fraction);
            (0..AMBIENT_PLACEMENT_TRIES).find_map(|_| {
                let random = &mut cadence.random;
                let point = bounds.min
                    + (bounds.max - bounds.min) * Vec2::new(random.unit_f32(), random.unit_f32());
                let heading = random.unit_f32() * std::f32::consts::TAU;
                let chunk = terrain_chunk_at(terrain, point)?;
                let (chunk, edited) = terrain_chunks.get(chunk).ok()?;
                let asset = terrain_assets.get(&chunk.asset)?;
                let sample = sample_authored_terrain(chunk, asset, edited, point)?;
                let biome = sample.biome?.id;
                let water = sample.geometry.water_height_m;
                let ambient_biome = sample
                    .water
                    .as_ref()
                    .and_then(|water| {
                        asset
                            .canonical_terrain_grid()
                            .biomes
                            .get(usize::from(water.biome_channel))
                    })
                    .map_or(biome, |biome| AssetId::from_key(&biome.name));
                let spawn = if definition.ambient.is_empty() {
                    select_listed_ambient(catalogue, audio, ambient_biome, water.is_some(), random)?
                } else {
                    let eligible: Vec<_> = definition
                        .ambient
                        .iter()
                        .filter_map(|&id| catalogue.find_ambient_spawn(id))
                        .filter(|row| {
                            row.biomes.contains(&biome)
                                && placement_accepts(row.class, water.is_some())
                                && daylight_fraction_unit_is_within_inclusive_window(
                                    phase,
                                    row.day_fraction,
                                )
                                && members.is_none_or(|members| {
                                    living
                                        .iter_many(members.iter())
                                        .flatten()
                                        .filter(|animal| animal.definition == row.id)
                                        .count()
                                        < usize::from(row.population[1])
                                })
                        })
                        .collect();
                    let row = eligible[random.range_u32(eligible.len() as u32)? as usize];
                    if !random_probability_threshold_passes(row.probability, random) {
                        return None;
                    }
                    let prefab = row.prefabs[random.range_u32(row.prefabs.len() as u32)? as usize];
                    AmbientSpawn {
                        definition: row.id,
                        object: None,
                        prefab: catalogue.scene(prefab)?,
                        class: row.class,
                        local_rules: Some(row),
                    }
                };
                let height = match spawn.class {
                    AmbientClass::Water => water?,
                    _ => sample.geometry.height_m,
                };
                Some((spawn, Vec3::new(point.x, height, point.y), heading))
            })
        } else {
            None
        };
        if let Some((spawn, position, heading)) = attempt {
            let expires = spawn.local_rules.map(|row| {
                clock
                    .tick
                    .saturating_add(u64::from(sample_inclusive_u32_range(
                        row.lifetime_ticks,
                        &mut cadence.random,
                    )))
            });
            let mode = match spawn.class {
                AmbientClass::Air => LocomotionMode::Flight,
                AmbientClass::Ground => LocomotionMode::Ground,
                AmbientClass::Water => LocomotionMode::Swim,
            };
            let mut animal = commands.spawn((
                AmbientAnimal {
                    definition: spawn.definition,
                    despawn_tick: expires,
                },
                AmbientAnimalOfWorld(root),
                WorldMember { root },
                PrefabPresentation::new(spawn.prefab),
                mode,
                Transform::from_translation(position).with_rotation(Quat::from_rotation_y(heading)),
                Visibility::Inherited,
            ));
            if let Some(object) = spawn.object {
                animal.insert(DefinitionId(object));
            }
            if let Some(row) = spawn.local_rules {
                if row.audio != AssetId::default() {
                    animal.insert(AmbientAudioEmitter {
                        definition: row.id,
                        cue: row.audio,
                        selection: (u64::from(cadence.random.next_u32()) << 32)
                            | u64::from(cadence.random.next_u32()),
                    });
                }
                if let Some(effect) = catalogue.effect(row.effect_asset) {
                    effects.write(SpawnParticleEffectEmitter {
                        particle_effect_asset: effect,
                        emitter_id: row.effect_emitter,
                        parent_entity: Some(animal.id()),
                        effect_transform: Transform::IDENTITY,
                        manual_particle_count: 0,
                    });
                }
            }
        }
        let [minimum, maximum] = policy.delay_seconds;
        let seconds = minimum + (maximum - minimum) * cadence.random.unit_f32();
        cadence
            .timer
            .set_duration(Duration::try_from_secs_f32(seconds).unwrap_or_default());
        cadence.timer.reset();
    }
}

fn placement_accepts(class: AmbientClass, water: bool) -> bool {
    match class {
        AmbientClass::Air => true,
        AmbientClass::Ground => !water,
        AmbientClass::Water => water,
    }
}

fn select_listed_ambient<'a>(
    catalogue: WorldDefinitionsView<'a>,
    audio: AudioView<'_>,
    biome: AssetId,
    water: bool,
    random: &mut DeterministicRng,
) -> Option<AmbientSpawn<'a>> {
    let allowed = audio.soundscape_allowed_entities(biome)?;
    let selected =
        allowed.allowed_entities[random.range_u32(allowed.allowed_entities.len() as u32)? as usize];
    let object = catalogue.find_object(selected).or_else(|| {
        catalogue.objects().find(|object| {
            catalogue
                .object_record_key(object.id)
                .map(AssetId::from_key)
                == Some(selected)
        })
    })?;
    let class = [
        ("ambient_air", AmbientClass::Air),
        ("ambient_water", AmbientClass::Water),
        ("ambient_ground", AmbientClass::Ground),
        ("ambient", AmbientClass::Ground),
    ]
    .into_iter()
    .find_map(|(kind, class)| {
        object
            .behavior_type_identifiers
            .contains(&AssetId::from_key(kind))
            .then_some(class)
    })?;
    placement_accepts(class, water).then_some(())?;
    Some(AmbientSpawn {
        definition: object.id,
        object: Some(object.id),
        prefab: catalogue.scene(object.prefab)?,
        class,
        local_rules: None,
    })
}

pub(super) fn retire_expired_world_ambient_animals(
    mut commands: Commands,
    worlds: Query<(&ZooClock, &WorldTerrainHorizontalBounds)>,
    animals: Query<(Entity, &AmbientAnimal, &AmbientAnimalOfWorld, &Transform)>,
) {
    for (entity, animal, owner, transform) in &animals {
        let Ok((clock, bounds)) = worlds.get(owner.0) else {
            continue;
        };
        let point = transform.translation.xz();
        if animal.despawn_tick.is_some_and(|end| clock.tick >= end)
            || point.cmplt(bounds.min).any()
            || point.cmpgt(bounds.max).any()
        {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn retire_effects_attached_to_removed_ambient_animal(
    removed: On<Remove<AmbientAnimal>>,
    mut effects: MessageWriter<StopParticleEffectsAttachedToEntity>,
) {
    effects.write(StopParticleEffectsAttachedToEntity {
        attached_entity: removed.entity,
        effect_asset: None,
    });
}

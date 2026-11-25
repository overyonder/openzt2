use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

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
    physics::fitting_surface_types::TerrainFittingSurface,
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
    animal_behavior::behavior_set_start_request_types::AmbientBehaviorSubject,
    animal_lifecycle::types::Age,
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

#[derive(Component, Clone, Default, PartialEq)]
pub(super) struct WorldAmbientCandidates {
    biomes: BTreeMap<AssetId, [Vec<AssetId>; 2]>,
    listed_air: Vec<AssetId>,
    unlisted: [Vec<AssetId>; 3],
}

pub(super) fn reconcile_world_ambient_candidates(
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    audio: Res<AudioAssets>,
    audio_assets: Res<Assets<AudioAsset>>,
    mut worlds: Query<&mut WorldAmbientCandidates>,
) {
    let sources_changed = definitions.is_changed()
        || definition_assets.is_changed()
        || audio.is_changed()
        || audio_assets.is_changed();
    let mut resolved = None;
    for mut candidates in &mut worlds {
        if !sources_changed && !candidates.is_added() {
            continue;
        }
        let next = resolved.get_or_insert_with(|| {
            let catalogue = definitions.get(&definition_assets)?;
            let audio = audio.get(&audio_assets)?;
            Some(build_ambient_candidates(catalogue, audio))
        });
        candidates.set_if_neq(
            next.as_ref()
                .map_or_else(WorldAmbientCandidates::default, Clone::clone),
        );
    }
}

fn build_ambient_candidates(
    definitions: WorldDefinitionsView<'_>,
    audio: AudioView<'_>,
) -> WorldAmbientCandidates {
    let mut candidates = WorldAmbientCandidates::default();
    let mut by_key = BTreeMap::new();
    let mut ambient_objects = Vec::new();
    for object in definitions.objects() {
        let Some(class) = ambient_class(object) else {
            continue;
        };
        by_key.insert(object.id, (object.id, class));
        if let Some(key) = definitions.object_record_key(object.id) {
            by_key.insert(AssetId::from_key(key), (object.id, class));
        }
        ambient_objects.push((object.id, class));
    }
    let mut listed = BTreeSet::new();
    let mut air = BTreeSet::new();
    for biome in definitions.biomes() {
        let Some(allowed) = audio.soundscape_allowed_entities(biome.id) else {
            continue;
        };
        let mut local = [Vec::new(), Vec::new()];
        for (id, class) in allowed
            .allowed_entities
            .iter()
            .filter_map(|key| by_key.get(key))
            .copied()
        {
            listed.insert(id);
            match class {
                AmbientClass::Air if air.insert(id) => candidates.listed_air.push(id),
                AmbientClass::Ground => local[0].push(id),
                AmbientClass::Water => local[1].push(id),
                AmbientClass::Air => {}
            }
        }
        candidates.biomes.insert(biome.id, local);
    }
    for (id, class) in ambient_objects {
        if !listed.contains(&id) {
            candidates.unlisted[match class {
                AmbientClass::Ground => 0,
                AmbientClass::Water => 1,
                AmbientClass::Air => 2,
            }]
            .push(id);
        }
    }
    candidates
}

struct AmbientSpawn<'a> {
    definition: AssetId,
    object: Option<AssetId>,
    prefab: Handle<ScenePrefabAsset>,
    scale: f32,
    fitting: Option<TerrainFittingSurface>,
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
        &WorldAmbientCandidates,
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
    for (root, clock, terrain, bounds, owned_environments, candidates, members, mut cadence) in
        &mut worlds
    {
        if clock.tick != tick || !policy.enabled {
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
        let admitted = cadence
            .random
            .range_u32(1000)
            .is_some_and(|roll| roll <= (policy.probability * 1000.0) as u32)
            && members.map_or(0, RelationshipTarget::len) < usize::from(policy.voice_limit);
        let phase = encode_daylight_fraction_as_u16(daylight.fraction);
        let mut next_interval_ticks = None;
        let attempt = admitted
            .then(|| {
                let row = if definition.ambient.is_empty() {
                    None
                } else {
                    let eligible: Vec<_> = definition
                        .ambient
                        .iter()
                        .filter_map(|id| catalogue.find_ambient_spawn(*id))
                        .filter(|row| {
                            daylight_fraction_unit_is_within_inclusive_window(
                                phase,
                                row.day_fraction,
                            ) && members.is_none_or(|members| {
                                living
                                    .iter_many(members.iter())
                                    .flatten()
                                    .filter(|animal| animal.definition == row.id)
                                    .count()
                                    < usize::from(row.population[1])
                            })
                        })
                        .collect();
                    let row = eligible[cadence.random.range_u32(eligible.len() as u32)? as usize];
                    next_interval_ticks = Some(sample_inclusive_u32_range(
                        row.spawn_interval_ticks,
                        &mut cadence.random,
                    ));
                    if !random_probability_threshold_passes(row.probability, &mut cadence.random) {
                        return None;
                    }
                    Some(row)
                };
                (0..AMBIENT_PLACEMENT_TRIES).find_map(|_| {
                    let random = &mut cadence.random;
                    let point = if let Some((row, origin)) =
                        row.and_then(|row| row.spawn_origin_cm.map(|origin| (row, origin)))
                    {
                        let angle = random.unit_f32() * std::f32::consts::TAU;
                        let radius =
                            sample_inclusive_u32_range(row.radius_cm, random) as f32 * 0.01;
                        Vec2::new(origin[0] as f32, origin[1] as f32) * 0.01
                            + Vec2::from_angle(angle) * radius
                    } else {
                        bounds.min
                            + (bounds.max - bounds.min)
                                * Vec2::new(random.unit_f32(), 1.0 - random.unit_f32())
                    };
                    let heading = random.unit_f32() * std::f32::consts::TAU;
                    let chunk = terrain_chunk_at(terrain, point)?;
                    let (chunk, edited) = terrain_chunks.get(chunk).ok()?;
                    let asset = terrain_assets.get(&chunk.asset)?;
                    let sample = sample_authored_terrain(chunk, asset, edited, point)?;
                    let biome = sample.biome?.id;
                    let water = sample.geometry.water_height_m;
                    let spawn = if let Some(row) = row {
                        if !row.biomes.contains(&biome)
                            || !placement_accepts(row.class, water.is_some())
                        {
                            return None;
                        }
                        let prefab =
                            row.prefabs[random.range_u32(row.prefabs.len() as u32)? as usize];
                        AmbientSpawn {
                            definition: row.id,
                            object: None,
                            prefab: catalogue.scene(prefab)?,
                            scale: 1.0,
                            fitting: None,
                            class: row.class,
                            local_rules: Some(row),
                        }
                    } else {
                        select_listed_ambient(
                            catalogue,
                            candidates,
                            biome,
                            water.is_some(),
                            random,
                        )?
                    };
                    let (point, heading) = if spawn.local_rules.is_none()
                        && matches!(spawn.class, AmbientClass::Air)
                    {
                        launch_air_ambient_from_map_edge(bounds, random)
                    } else {
                        (point, heading)
                    };
                    // The authored entity's ground fitter places it vertically at
                    // the launch site. A custom placement can request water support directly.
                    let height = match (spawn.local_rules, spawn.class) {
                        (None, _) => 0.0,
                        (Some(_), AmbientClass::Water) => water?,
                        (Some(_), _) => sample.geometry.height_m,
                    };
                    Some((spawn, Vec3::new(point.x, height, point.y), heading))
                })
            })
            .flatten();
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
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(heading - std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(spawn.scale)),
                Visibility::Inherited,
            ));
            if let Some(object) = spawn.object {
                animal.insert((
                    DefinitionId(object),
                    AmbientBehaviorSubject,
                    Age { born_tick: tick },
                ));
            }
            if let Some(fitting) = spawn.fitting {
                animal.insert(fitting);
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
        let next_interval = if let Some(ticks) = next_interval_ticks {
            timing.timestep().saturating_mul(ticks)
        } else {
            let seconds = minimum + (maximum - minimum) * cadence.random.unit_f32();
            Duration::try_from_secs_f32(seconds).unwrap_or_default()
        };
        cadence.timer.set_duration(next_interval);
        cadence.timer.reset();
    }
}

fn launch_air_ambient_from_map_edge(
    bounds: &WorldTerrainHorizontalBounds,
    random: &mut DeterministicRng,
) -> (Vec2, f32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let edge = random.range_u32(4).expect("four map edges");
    let heading = PI * (0.2 + 0.6 * random.unit_f32());
    let along = random.unit_f32();
    let point = bounds.min
        + (bounds.max - bounds.min)
            * match edge {
                0 => Vec2::new(along, 1.0),
                1 => Vec2::new(0.0, 1.0 - along),
                2 => Vec2::new(along, 0.0),
                _ => Vec2::new(1.0, 1.0 - along),
            };
    let rotation = [0.0, -FRAC_PI_2, PI, FRAC_PI_2][edge as usize];
    (point, heading + rotation)
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
    candidates: &WorldAmbientCandidates,
    biome: AssetId,
    water: bool,
    random: &mut DeterministicRng,
) -> Option<AmbientSpawn<'a>> {
    let local = candidates
        .biomes
        .get(&biome)
        .map_or(&[][..], |choices| &choices[usize::from(water)]);
    let [ground, water_only, air] = candidates.unlisted.each_ref();
    let groups: [&[AssetId]; 4] = if water {
        [local, water_only, &candidates.listed_air, air]
    } else {
        [local, &candidates.listed_air, air, ground]
    };
    let count = groups.iter().map(|group| group.len() as u32).sum();
    let index = random.range_u32(count)? as usize;
    let selected = groups.into_iter().flatten().nth(index)?;
    let object = catalogue.find_object(*selected)?;
    Some(AmbientSpawn {
        definition: object.id,
        object: Some(object.id),
        prefab: catalogue.scene(object.prefab)?,
        scale: object.prefab_scale,
        fitting: object.terrain_fitted.then_some(TerrainFittingSurface {
            normal_tolerance: 0.0,
            height_offset_m: object.terrain_fit_height_offset_m,
            water_offset_m: object.terrain_fit_water_offset_m,
        }),
        class: ambient_class(object)?,
        local_rules: None,
    })
}

fn ambient_class(
    object: &openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition,
) -> Option<AmbientClass> {
    [
        ("ambient_water", AmbientClass::Water),
        ("ambient_air", AmbientClass::Air),
        ("ambient_ground", AmbientClass::Ground),
    ]
    .into_iter()
    .find_map(|(kind, class)| {
        object
            .behavior_type_identifiers
            .contains(&AssetId::from_key(kind))
            .then_some(class)
    })
}

pub(super) fn retire_expired_world_ambient_animals(
    mut commands: Commands,
    worlds: Query<&ZooClock>,
    animals: Query<(Entity, &AmbientAnimal, &AmbientAnimalOfWorld)>,
) {
    for (entity, animal, owner) in &animals {
        let Ok(clock) = worlds.get(owner.0) else {
            continue;
        };
        if animal.despawn_tick.is_some_and(|end| clock.tick >= end) {
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

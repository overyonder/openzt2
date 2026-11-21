use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{Duration, Instant},
};

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    asset_source::AssetArchives,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_game_data::{
    species::{LifeStage, Sex, SpeciesFlags},
    AssetId,
};
use openzt2_world_state::world_spawn::{
    prefab_presentation_types::PrefabModel, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::{
    animal_lifecycle::{
        animal_adoption_contracts::{AdoptAnimal, AnimalAdopted},
        types::{AnimalPresentationOwner, AnimalVariant},
    },
    terrain::{terrain_chunk_types::TerrainChunk, terrain_world_sampling::sample_terrain},
};

use super::ArchiveNames;

#[derive(Resource, Default)]
pub(super) struct AnimalBatch {
    trials: Vec<Trial>,
    started: Option<Instant>,
    finished: bool,
}

impl AnimalBatch {
    pub(super) fn finished(&self) -> bool {
        self.finished
    }
}

struct Trial {
    archive: String,
    species: AssetId,
    variant: AssetId,
    sex: Sex,
    model: String,
    animal: Option<Entity>,
    model_ready: bool,
    animation_verified: bool,
}

#[derive(SystemParam)]
struct Inputs<'w, 's> {
    worlds: Query<'w, 's, Entity, With<WorldRoot>>,
    chunks: Query<'w, 's, &'static TerrainChunk>,
    terrain: Res<'w, Assets<TerrainAsset>>,
    species: Res<'w, SpeciesAssets>,
    species_assets: Res<'w, Assets<SpeciesAsset>>,
    archives: Res<'w, AssetArchives>,
    names: Res<'w, ArchiveNames>,
}

pub(super) fn install(app: &mut App) {
    if std::env::var_os("OPENZT2_MOD_AUDIT_ANIMAL_BATCH").is_some() {
        app.init_resource::<AnimalBatch>()
            .add_systems(Last, exercise.run_if(in_state(GamePhase::InGame)));
    }
}

fn exercise(
    mut batch: ResMut<AnimalBatch>,
    inputs: Inputs,
    mut requests: MessageWriter<AdoptAnimal>,
    mut adopted: MessageReader<AnimalAdopted>,
    variants: Query<&AnimalVariant>,
    models: Query<(&AnimalPresentationOwner, &PrefabModel)>,
    playback: Query<&AnimationPlayer>,
    server: Res<AssetServer>,
) {
    if batch.finished {
        return;
    }
    let Some(started) = batch.started else {
        let Some(trials) = start_trials(&inputs, &mut requests) else {
            return;
        };
        println!("mod-audit\tanimal-batch-started\t{}", trials.len());
        batch.trials = trials;
        batch.started = Some(Instant::now());
        return;
    };

    for event in adopted.read() {
        let Ok(variant) = variants.get(event.animal) else {
            continue;
        };
        if let Some(trial) = batch
            .trials
            .iter_mut()
            .find(|trial| trial.variant == variant.0 && trial.species == event.species)
        {
            trial.animal = Some(event.animal);
        }
    }
    for trial in &mut batch.trials {
        let Some(animal) = trial.animal else {
            continue;
        };
        trial.model_ready |= models.iter().any(|(owner, model)| {
            owner.0 == animal
                && model
                    .handle
                    .as_ref()
                    .is_some_and(|handle| server.is_loaded_with_dependencies(handle.id()))
        });
        trial.animation_verified |= playback.get(animal).is_ok_and(|player| {
            player
                .playing_animations()
                .any(|(_, active)| active.seek_time() > 0.0)
        });
    }

    if started.elapsed() >= Duration::from_secs(30)
        || batch
            .trials
            .iter()
            .all(|trial| trial.model_ready && trial.animation_verified)
    {
        for trial in &batch.trials {
            let result = match (trial.animal, trial.model_ready, trial.animation_verified) {
                (Some(_), true, true) => "passed",
                (Some(_), true, false) => "animation not observed",
                (Some(_), false, _) => "model not ready",
                (None, _, _) => "adoption not completed",
            };
            println!(
                "mod-audit\tanimal-result\t{}\t{result}\t{}",
                trial.archive, trial.model
            );
        }
        batch.finished = true;
    }
}

fn start_trials(inputs: &Inputs, requests: &mut MessageWriter<AdoptAnimal>) -> Option<Vec<Trial>> {
    let world = inputs.worlds.single().ok()?;
    let species_index = inputs.species.get(&inputs.species_assets)?;
    let mut positions = inputs
        .chunks
        .iter()
        .filter_map(|chunk| {
            let asset = inputs.terrain.get(&chunk.asset)?;
            let xz = chunk.origin + Vec2::splat(f32::from(chunk.side / 2) * chunk.spacing_m);
            let sample = sample_terrain(chunk, asset, None, xz)?;
            sample
                .water_height_m
                .is_none()
                .then_some(Vec3::new(xz.x, sample.height_m, xz.y))
        })
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return None;
    }
    positions.sort_by(|left, right| {
        left.length_squared()
            .total_cmp(&right.length_squared())
            .then(left.x.total_cmp(&right.x))
            .then(left.z.total_cmp(&right.z))
    });

    let mut seen_species = BTreeSet::new();
    let mut trials = BTreeMap::new();
    for species in species_index.species().filter(|species| {
        seen_species.insert(species.id) && species.flags.contains(SpeciesFlags::ADOPTABLE)
    }) {
        for variant in species_index.variants(species.id).filter(|variant| {
            variant.life_stage == LifeStage::Adult
                && variant.placement_surface_policy.land
                && variant.sex != Sex::Any
                && variant.probability != 0
        }) {
            let Some(entry) = inputs
                .archives
                .resolved_winning_entry(Path::new(&variant.model))
            else {
                continue;
            };
            let Some(archive) = inputs
                .names
                .0
                .get(entry.archive_priority_index)
                .and_then(|path| path.to_str())
            else {
                continue;
            };
            if z2f::REQUIRED_ARCHIVES.contains(&archive) {
                continue;
            }
            trials.entry(archive).or_insert_with(|| Trial {
                archive: archive.to_owned(),
                species: species.id,
                variant: variant.id,
                sex: variant.sex,
                model: variant.model.clone(),
                animal: None,
                model_ready: false,
                animation_verified: false,
            });
        }
    }
    let trials = trials.into_values().collect::<Vec<_>>();
    for (trial, position) in trials.iter().zip(positions.iter().cycle()) {
        requests.write(AdoptAnimal {
            species: trial.species,
            variant: Some(trial.variant),
            sex: Some(trial.sex),
            offer_slot_index: None,
            world,
            habitat: None,
            transform: Transform::from_translation(*position),
        });
    }
    Some(trials)
}

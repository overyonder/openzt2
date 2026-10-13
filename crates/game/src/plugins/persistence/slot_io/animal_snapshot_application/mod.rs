use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    animal_snapshot_types::AnimalSnapshotRecord,
};
use crate::plugins::{
    animal_health::types::{Dead, DeathCause},
    animal_lifecycle::{
        animal_entity_spawning::spawn_animal_entity_from_resolved_components,
        animal_spawn_resolution::lower_species_and_variant_to_animal_spawn_components,
        animal_spawn_resolution_types::ResolvedAnimalSpawnComponents, types::*,
    },
    animal_welfare::types::*,
    simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        world_entity_crating::WorldEntityIsCrated,
        world_membership_types::WorldMember,
    },
};
use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use std::collections::BTreeMap;

pub(super) fn resolve_saved_animal_components(
    records: &[AnimalSnapshotRecord],
    species: SpeciesView<'_>,
    ticks_per_day: u32,
) -> Result<Vec<ResolvedAnimalSpawnComponents>, WorldSnapshotPersistenceFailure> {
    records
        .iter()
        .map(|record| {
            let (definition, variant) = species
                .find_variant_with_species(record.variant)
                .filter(|(definition, _)| definition.id == record.species)
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
            if variant.sex != openzt2_game_data::species::Sex::Any && variant.sex != record.sex {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            // Restoration keeps the saved scale; only authored static facts are hydrated here.
            let mut rng = DeterministicRng::from_entity(
                ZooSeed(0),
                PersistentId(record.persistent_identifier),
                RngDomain::Reproduction,
            );
            let mut resolved = lower_species_and_variant_to_animal_spawn_components(
                definition,
                variant,
                record.sex,
                record.life_stage,
                ticks_per_day,
                &mut rng,
            )
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
            resolved.scale = 1.0;
            Ok(resolved)
        })
        .collect()
}

pub(super) fn apply_animal_snapshot_records(
    commands: &mut Commands,
    records: Vec<AnimalSnapshotRecord>,
    resolved_components: Vec<ResolvedAnimalSpawnComponents>,
    world_root: Entity,
    allocator: &mut PersistentIdAllocator,
    baseline_animals: &Query<Entity, With<Animal>>,
) -> Result<BTreeMap<u64, Entity>, WorldSnapshotPersistenceFailure> {
    // Reserve every identifier before queuing any entity mutations.
    for record in &records {
        allocator
            .reserve_imported(world_root, PersistentId(record.persistent_identifier))
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    }
    for animal in baseline_animals {
        commands.entity(animal).despawn();
    }
    let mut entities = BTreeMap::new();
    for (record, resolved) in records.iter().zip(resolved_components) {
        let animal = spawn_animal_entity_from_resolved_components(
            commands,
            record.species,
            resolved,
            PersistentId(record.persistent_identifier),
            WorldMember { root: world_root },
            None,
            Transform {
                translation: Vec3::from_array(record.translation),
                rotation: Quat::from_array(record.rotation),
                scale: Vec3::from_array(record.scale),
            },
            Parents::NONE,
            record.born_tick,
        );
        entities.insert(record.persistent_identifier, animal);
        let mut entity = commands.entity(animal);
        if let Some(name) = &record.name {
            entity.insert(Name::new(name.clone()));
        }
        if record.crated {
            entity.insert(WorldEntityIsCrated);
        }
        if record.release_restricted {
            entity.insert(ReleaseRestricted);
        }
        if record.crating_restricted {
            entity.insert(CratingRestricted);
        }
        if let Some((disease, since_tick)) = record.death {
            entity.insert(Dead {
                cause: DeathCause::Disease(disease),
                since_tick,
            });
        }
        if let Some(welfare) = &record.welfare {
            let values = welfare.needs;
            entity.insert((
                Hunger(values[0]),
                Thirst(values[1]),
                RestNeed(values[2]),
                PrivacyNeed(values[3]),
                SocialNeed(values[4]),
                ExerciseNeed(values[5]),
                StimulationNeed(values[6]),
                EnvironmentNeed(values[7]),
                HealthNeed(values[8]),
                HygieneNeed(values[9]),
                BathroomNeed(values[10]),
            ));
            entity.insert((
                NeedTriggerState(welfare.triggers),
                AnimalWelfare(welfare.welfare),
                CumulativeNeedPoints {
                    good: welfare.cumulative_points[0],
                    bad: welfare.cumulative_points[1],
                },
                HabitatSuitability {
                    space: welfare.habitat_suitability[0],
                    biome: welfare.habitat_suitability[1],
                    overall: welfare.habitat_suitability[2],
                },
                match welfare.band {
                    0 => WelfareBand::Critical,
                    1 => WelfareBand::Poor,
                    2 => WelfareBand::Fair,
                    3 => WelfareBand::Good,
                    _ => WelfareBand::Excellent,
                },
            ));
        }
    }
    // Every animal exists before resolving lineage, independent of record ordering.
    for record in records {
        let mut animal = commands.entity(entities[&record.persistent_identifier]);
        animal.insert(Parents {
            mother: record.parents[0].map(|id| entities[&id]),
            father: record.parents[1].map(|id| entities[&id]),
        });
        if let Some((father, due_tick)) = record.pregnancy {
            animal.insert(Pregnancy {
                father: entities[&father],
                due_tick,
            });
        }
    }
    Ok(entities)
}

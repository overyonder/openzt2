use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    animal_snapshot_section_encoding::validate_animal_snapshot_records,
    animal_snapshot_types::{AnimalSnapshotRecord, AnimalWelfareSnapshotRecord},
};
use crate::plugins::{
    animal_health::types::{Dead, DeathCause},
    animal_lifecycle::types::*,
    animal_welfare::types::*,
    world_spawn::{persistent_id_types::PersistentId, world_entity_crating::WorldEntityIsCrated},
};
use bevy::{ecs::resource::IsResource, prelude::*};

/// Whole animal entities. Resources are entities too, so exclude them to keep
/// this read access disjoint from resource writes in the same system.
pub(in crate::plugins::persistence) type AnimalSnapshotEntities<'w, 's> =
    Query<'w, 's, EntityRef<'static>, (With<Animal>, Without<IsResource>)>;

pub(super) fn capture_animal_snapshot_records(
    animals: &AnimalSnapshotEntities,
) -> Result<Vec<AnimalSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let persistent_reference = |entity| {
        animals
            .get(entity)
            .ok()
            .and_then(|animal| animal.get::<PersistentId>().map(|id| id.0))
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
    };
    let mut records = Vec::with_capacity(animals.iter().len());
    for animal in animals {
        let transform = required_animal_component::<Transform>(animal)?;
        let parents = required_animal_component::<Parents>(animal)?;
        let welfare = if let Some(welfare) = animal.get::<AnimalWelfare>() {
            Some(AnimalWelfareSnapshotRecord {
                needs: [
                    required_animal_component::<Hunger>(animal)?.0,
                    required_animal_component::<Thirst>(animal)?.0,
                    required_animal_component::<RestNeed>(animal)?.0,
                    required_animal_component::<PrivacyNeed>(animal)?.0,
                    required_animal_component::<SocialNeed>(animal)?.0,
                    required_animal_component::<ExerciseNeed>(animal)?.0,
                    required_animal_component::<StimulationNeed>(animal)?.0,
                    required_animal_component::<EnvironmentNeed>(animal)?.0,
                    required_animal_component::<HealthNeed>(animal)?.0,
                    required_animal_component::<HygieneNeed>(animal)?.0,
                    required_animal_component::<BathroomNeed>(animal)?.0,
                ],
                triggers: required_animal_component::<NeedTriggerState>(animal)?.0,
                cumulative_points: {
                    let points = required_animal_component::<CumulativeNeedPoints>(animal)?;
                    [points.good, points.bad]
                },
                welfare: welfare.0,
                habitat_suitability: {
                    let suitability = required_animal_component::<HabitatSuitability>(animal)?;
                    [suitability.space, suitability.biome, suitability.overall]
                },
                band: match required_animal_component::<WelfareBand>(animal)? {
                    WelfareBand::Critical => 0,
                    WelfareBand::Poor => 1,
                    WelfareBand::Fair => 2,
                    WelfareBand::Good => 3,
                    WelfareBand::Excellent => 4,
                },
            })
        } else {
            None
        };
        records.push(AnimalSnapshotRecord {
            persistent_identifier: required_animal_component::<PersistentId>(animal)?.0,
            species: required_animal_component::<SpeciesHandle>(animal)?.species,
            variant: required_animal_component::<AnimalVariant>(animal)?.0,
            sex: required_animal_component::<AnimalSex>(animal)?.0,
            life_stage: required_animal_component::<AnimalLifeStage>(animal)?.0,
            born_tick: required_animal_component::<Age>(animal)?.born_tick,
            name: animal.get::<Name>().map(|name| name.as_str().to_owned()),
            translation: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            scale: transform.scale.to_array(),
            parents: [
                parents.mother.map(persistent_reference).transpose()?,
                parents.father.map(persistent_reference).transpose()?,
            ],
            pregnancy: animal
                .get::<Pregnancy>()
                .map(|pregnancy| {
                    persistent_reference(pregnancy.father)
                        .map(|father| (father, pregnancy.due_tick))
                })
                .transpose()?,
            crated: animal.contains::<WorldEntityIsCrated>(),
            release_restricted: animal.contains::<ReleaseRestricted>(),
            crating_restricted: animal.contains::<CratingRestricted>(),
            death: animal.get::<Dead>().map(|dead| match dead.cause {
                DeathCause::Disease(disease) => (disease, dead.since_tick),
            }),
            welfare,
        });
    }
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    validate_animal_snapshot_records(&records)?;
    Ok(records)
}

fn required_animal_component<T: Component>(
    animal: EntityRef<'_>,
) -> Result<&T, WorldSnapshotPersistenceFailure> {
    animal
        .get::<T>()
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}

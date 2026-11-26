use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    animal_snapshot_section_encoding::validate_animal_snapshot_records,
    animal_snapshot_types::{
        AnimalPendingTokenSnapshotRecord, AnimalSnapshotDeath, AnimalSnapshotRecord,
        AnimalWelfareSnapshotRecord,
    },
};
use bevy::{ecs::resource::IsResource, prelude::*};
use openzt2_animals::animal_welfare::need_timing_types::NeedAdjustmentCadence;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_entity_crating::{AnimalCrate, ScriptedCrating};
use openzt2_zoo_state::animal_behavior::animal_lineage::{
    AnimalFather, AnimalMate, AnimalMother, PregnancyFather,
};
use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, ClaimedBehaviorSelfToken,
};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::DeathCause;
use openzt2_zoo_state::animal_lifecycle::types::*;
use openzt2_zoo_state::animal_welfare::types::*;
use openzt2_zoo_state::locomotion::locomotion_types::CaffeineFlags;

/// Whole animal entities. Resources are entities too, so exclude them to keep
/// this read access disjoint from resource writes in the same system.
pub(in crate::persistence) type AnimalSnapshotEntities<'w, 's> =
    Query<'w, 's, EntityRef<'static>, (With<Animal>, Without<IsResource>)>;

/// Tokens no running task has claimed yet.
pub(in crate::persistence) type PendingTokenSnapshotEntities<'w, 's> = Query<
    'w,
    's,
    (&'static BehaviorSelfToken, &'static ChildOf),
    Without<ClaimedBehaviorSelfToken>,
>;

pub(super) fn capture_animal_snapshot_records(
    animals: &AnimalSnapshotEntities,
    tokens: &PendingTokenSnapshotEntities,
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
        let welfare = if let Some(welfare) = animal.get::<AnimalWelfare>() {
            Some(AnimalWelfareSnapshotRecord {
                need_adjustment_timer: animal
                    .get::<NeedAdjustmentCadence>()
                    .map(NeedAdjustmentCadence::saved_remaining),
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
                reproduction_need: animal.get::<ReproductionNeed>().map(|need| need.0),
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
                environment_habitat_assessed: animal.contains::<EnvironmentHabitatAssessed>(),
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
                animal
                    .get::<AnimalMother>()
                    .map(|link| persistent_reference(link.0))
                    .transpose()?,
                animal
                    .get::<AnimalFather>()
                    .map(|link| persistent_reference(link.0))
                    .transpose()?,
            ],
            pregnancy: animal
                .get::<PregnancyFather>()
                .map(|father| persistent_reference(father.0).map(|father| (father, 0)))
                .transpose()?,
            pregnant_without_father: animal.contains::<Pregnancy>()
                && !animal.contains::<PregnancyFather>(),
            mate: animal
                .get::<AnimalMate>()
                .map(|mate| persistent_reference(mate.0))
                .transpose()?,
            pending_tokens: {
                let mut pending = Vec::new();
                for (token, holder) in tokens {
                    if holder.parent() != animal.id() {
                        continue;
                    }
                    pending.push(AnimalPendingTokenSnapshotRecord {
                        name: token.name,
                        available_at: token.available_at,
                        expires_at: token.expires_at,
                        // A payload that left the zoo no longer names anyone.
                        payload: token
                            .payload
                            .and_then(|payload| persistent_reference(payload).ok()),
                        retain_on_failure: token.retain_on_failure,
                    });
                }
                pending.sort_unstable_by_key(|token| (token.available_at, token.name.0));
                pending
            },
            crated: (animal.contains::<AnimalCrate>() || animal.contains::<ScriptedCrating>()),
            crate_identifier: None,
            release_restricted: animal.contains::<ReleaseRestricted>(),
            caffeinated: animal.get::<CaffeineFlags>().map(|flags| flags.caffeinated),
            decaffeinated: animal
                .get::<CaffeineFlags>()
                .map(|flags| flags.decaffeinated),
            crating_restricted: animal.contains::<CratingRestricted>(),
            death: animal.get::<Dead>().map(|dead| match dead.cause {
                DeathCause::Disease(disease) => {
                    AnimalSnapshotDeath::Disease(disease, dead.since_tick)
                }
                DeathCause::Authored(program) => AnimalSnapshotDeath::Authored {
                    program,
                    since_tick: dead.since_tick,
                },
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

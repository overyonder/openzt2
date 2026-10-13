use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::behavior::{
    action::modification::BehaviorFact, action_record::BehaviorAction,
};

use crate::plugins::{
    animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    animal_welfare::types::{AdjustThirst, Thirst},
    behavior_task_execution_types::{
        find_current_behavior_task_action, BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
        BehaviorTaskFinished, PendingBehaviorAnimationClipCompletion, PendingBehaviorTaskFailure,
    },
    feeding::container_quantity::DrinkContainer,
    habitat::habitat_types::HabitatMember,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{persistent_id_types::PersistentId, world_membership_types::DefinitionId},
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationFacts, VerificationJourneyReport,
    VerificationJourneySession, VerificationSessionStage,
};

#[derive(QueryData)]
struct DrinkingAnimal {
    entity: Entity,
    species: &'static SpeciesHandle,
    thirst: &'static Thirst,
    task: Option<&'static BehaviorTaskExecutionState>,
    animation: Option<&'static PendingBehaviorAnimationClipCompletion>,
    failure: Option<&'static PendingBehaviorTaskFailure>,
    habitat: &'static HabitatMember,
}

#[derive(QueryData)]
struct DrinkingSource {
    entity: Entity,
    quantity: &'static DrinkContainer,
    definition: &'static DefinitionId,
    persistent: &'static PersistentId,
    habitat: &'static HabitatMember,
}

#[derive(SystemParam)]
pub(super) struct AnimalDrinkingQueries<'w, 's> {
    animals: Query<'w, 's, DrinkingAnimal, With<Animal>>,
    sources: Query<'w, 's, DrinkingSource>,
    documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    occupancy: Res<'w, InteractionContainerOccupancy>,
}

/// Fixed-update measurements of one terminal instruction, never gameplay inputs.
struct DrinkingTerminalMeasurement {
    actor: Entity,
    source: Entity,
    source_id: PersistentId,
    execution_id: u64,
    action: usize,
    quantity_before: i32,
    thirst_before: i32,
    docked: bool,
}

struct DrinkingUseEvidence {
    measurement: DrinkingTerminalMeasurement,
    quantity_after: i32,
    thirst_after: i32,
    applications: u32,
    thirst_adjustments: u32,
    thirst_adjustment_q16: i64,
    identity_matches: bool,
    finishes: u32,
}

#[derive(Resource, Default)]
pub(super) struct AnimalDrinkingEvidence {
    journey: Option<(usize, u64)>,
    pending: Option<DrinkingTerminalMeasurement>,
    first_use: Option<DrinkingUseEvidence>,
}

fn prepare_drinking_evidence(
    session: &VerificationJourneySession,
    evidence: &mut AnimalDrinkingEvidence,
) -> bool {
    if session.stage != VerificationSessionStage::Journey
        || !session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "populated-zoo")
    {
        return false;
    }
    let identity = session
        .journey_started_at
        .map(|(frame, _)| (session.journey_index, frame));
    if evidence.journey != identity {
        *evidence = AnimalDrinkingEvidence {
            journey: identity,
            ..default()
        };
    }
    true
}

fn persistent_source_id_fact_value(id: PersistentId) -> f64 {
    // Numeric journey comparisons must retain exact identity. Larger ids fail
    // the journey's positive-id precondition instead of rounding together.
    u32::try_from(id.0).map_or(-1.0, f64::from)
}

pub(super) fn observe_drinking_before_behavior_execution(
    session: Res<VerificationJourneySession>,
    mut evidence: ResMut<AnimalDrinkingEvidence>,
    world: AnimalDrinkingQueries,
    clock: Option<Res<ZooClock>>,
) {
    evidence.pending = None;
    if !prepare_drinking_evidence(&session, &mut evidence) {
        return;
    }
    let Some(clock) = clock else {
        return;
    };
    for animal in &world.animals {
        let Some(task) = animal.task else {
            continue;
        };
        if evidence
            .first_use
            .as_ref()
            .is_some_and(|record| record.measurement.execution_id != task.execution_id)
        {
            continue;
        }
        if task.phase != BehaviorTaskExecutionPhase::Completion
            || !task.stack.is_empty()
            || task.next_action_tick > clock.tick
            || animal.failure.is_some()
        {
            continue;
        }
        let Some(BehaviorAction::FactModifications(modifications)) = world
            .documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, task))
        else {
            continue;
        };
        if ![BehaviorFact::Thirst, BehaviorFact::FoodLevel]
            .into_iter()
            .all(|fact| {
                modifications
                    .iter()
                    .any(|modification| modification.modified_fact == fact)
            })
        {
            continue;
        }
        let Some(source) = task
            .target
            .and_then(|source| world.sources.get(source).ok())
        else {
            continue;
        };
        if source.definition.0 != source.quantity.drink {
            continue;
        }
        if let Some(declaration) = world
            .documents
            .get(&task.document)
            .and_then(|document| document.behavior_task_at_index(task.declaration))
        {
            info!(target: "openzt2_verification", execution = task.execution_id,
                document = ?task.document.path(), program = ?task.program,
                name = %declaration.name, subjects = ?declaration.subjects,
                "observing finite drink task completion");
        }
        evidence.pending = Some(DrinkingTerminalMeasurement {
            actor: animal.entity,
            source: source.entity,
            source_id: *source.persistent,
            execution_id: task.execution_id,
            action: task.action,
            quantity_before: source.quantity.amount_q16,
            thirst_before: animal.thirst.0,
            docked: world
                .occupancy
                .docked_member_entities_in_container(source.entity)
                .any(|actor| actor == animal.entity),
        });
        break;
    }
}

pub(super) fn observe_drinking_after_behavior_effects(
    session: Res<VerificationJourneySession>,
    mut evidence: ResMut<AnimalDrinkingEvidence>,
    world: AnimalDrinkingQueries,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut thirst_adjustments: MessageReader<AdjustThirst>,
) {
    if !prepare_drinking_evidence(&session, &mut evidence) {
        finished.clear();
        thirst_adjustments.clear();
        return;
    }
    if let Some(measurement) = evidence.pending.take() {
        let (adjustment_count, adjustment_q16) = thirst_adjustments
            .read()
            .filter(|adjustment| adjustment.animal == measurement.actor)
            .fold((0_u32, 0_i64), |(count, delta), adjustment| {
                (count + 1, delta + i64::from(adjustment.delta_q16))
            });
        let (Ok(animal), Ok(source)) = (
            world.animals.get(measurement.actor),
            world.sources.get(measurement.source),
        ) else {
            return;
        };
        let same_instruction_advanced = animal.task.is_some_and(|task| {
            task.execution_id == measurement.execution_id
                && task.target == Some(measurement.source)
                && task.phase == BehaviorTaskExecutionPhase::Completion
                && task.stack.is_empty()
                && task.action == measurement.action + 1
        });
        let has_observed_effects = same_instruction_advanced
            || source.quantity.amount_q16 != measurement.quantity_before
            || animal.thirst.0 != measurement.thirst_before
            || adjustment_count != 0;
        if has_observed_effects {
            let identity_matches = same_instruction_advanced
                && *source.persistent == measurement.source_id
                && source.definition.0 == source.quantity.drink;
            info!(target: "openzt2_verification", execution = measurement.execution_id,
            actor = ?measurement.actor, source = ?measurement.source, persistent_source = measurement.source_id.0,
            quantity_before_q16 = measurement.quantity_before, quantity_after_q16 = source.quantity.amount_q16,
            thirst_before_q16 = measurement.thirst_before, thirst_after_q16 = animal.thirst.0,
            docked = measurement.docked, identity_matches, adjustment_count, adjustment_q16,
            "finite drinking terminal effects");
            if let Some(previous) = &mut evidence.first_use {
                previous.applications += 1;
                previous.thirst_adjustments += adjustment_count;
                previous.thirst_adjustment_q16 += adjustment_q16;
                previous.identity_matches &= identity_matches;
            } else {
                evidence.first_use = Some(DrinkingUseEvidence {
                    measurement,
                    quantity_after: source.quantity.amount_q16,
                    thirst_after: animal.thirst.0,
                    applications: 1,
                    thirst_adjustments: adjustment_count,
                    thirst_adjustment_q16: adjustment_q16,
                    identity_matches,
                    finishes: 0,
                });
            }
        }
    }
    thirst_adjustments.clear();
    for completion in finished.read() {
        if let Some(use_evidence) = evidence.first_use.as_mut().filter(|record| {
            record.measurement.actor == completion.actor
                && record.measurement.execution_id == completion.execution_id
        }) {
            use_evidence.finishes += 1;
        }
    }
}

pub(super) fn collect_animal_drinking_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut evidence: ResMut<AnimalDrinkingEvidence>,
    world: AnimalDrinkingQueries,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected
        || !prepare_drinking_evidence(&session, &mut evidence)
    {
        return;
    }
    let sources = world.sources.iter().count();
    let aardvarks = world
        .animals
        .iter()
        .filter(|animal| animal.species.species == openzt2_game_data::AssetId::from_key("aardvark"))
        .count();
    let docked_animations = world
        .animals
        .iter()
        .filter(|animal| {
            animal.animation.is_some()
                && animal.task.is_some_and(|task| {
                    task.target.is_some_and(|source| {
                        world.sources.contains(source)
                            && world
                                .occupancy
                                .docked_member_entities_in_container(source)
                                .any(|actor| actor == animal.entity)
                    })
                })
        })
        .count();
    for (name, value) in [
        ("drinking_sources", sources),
        ("drinking_aardvarks", aardvarks),
        ("drinking_docked_animation_uses", docked_animations),
    ] {
        report.current_facts.insert(
            name.to_owned(),
            f64::from(u32::try_from(value).unwrap_or(u32::MAX)),
        );
    }
    if let Ok(source) = world.sources.single() {
        report.current_facts.insert(
            "drinking_source_id".to_owned(),
            persistent_source_id_fact_value(*source.persistent),
        );
        report.current_facts.insert(
            "drinking_source_quantity".to_owned(),
            f64::from(source.quantity.amount_q16) / 65_536.0,
        );
        report.current_facts.insert(
            "drinking_source_is_water_dish".to_owned(),
            f64::from(u32::from(
                source.definition.0 == openzt2_game_data::AssetId::from_key("fooddish_water")
                    && source.definition.0 == source.quantity.drink,
            )),
        );
        report.current_facts.insert(
            "drinking_source_shares_animal_habitat".to_owned(),
            f64::from(u32::from(world.animals.iter().any(|animal| {
                animal.habitat.habitat_entity == source.habitat.habitat_entity
            }))),
        );
    }
    collect_first_drinking_use_verification_facts(
        evidence.first_use.as_ref(),
        &mut report.current_facts,
    );
}

fn collect_first_drinking_use_verification_facts(
    first: Option<&DrinkingUseEvidence>,
    facts: &mut VerificationFacts,
) {
    for (name, value) in [
        (
            "drinking_first_applications",
            first.map_or(0.0, |record| f64::from(record.applications)),
        ),
        (
            "drinking_first_thirst_adjustments",
            first.map_or(0.0, |record| f64::from(record.thirst_adjustments)),
        ),
        (
            "drinking_first_thirst_adjustment",
            first.map_or(0.0, |record| {
                i32::try_from(record.thirst_adjustment_q16)
                    .map_or(-1.0, |delta| f64::from(delta) / 65_536.0)
            }),
        ),
        (
            "drinking_first_source_id",
            first.map_or(0.0, |record| {
                persistent_source_id_fact_value(record.measurement.source_id)
            }),
        ),
        (
            "drinking_first_quantity_before",
            first.map_or(0.0, |record| {
                f64::from(record.measurement.quantity_before) / 65_536.0
            }),
        ),
        (
            "drinking_first_quantity_after",
            first.map_or(0.0, |record| f64::from(record.quantity_after) / 65_536.0),
        ),
        (
            "drinking_first_thirst_before",
            first.map_or(0.0, |record| {
                f64::from(record.measurement.thirst_before) / 65_536.0
            }),
        ),
        (
            "drinking_first_thirst_after",
            first.map_or(0.0, |record| f64::from(record.thirst_after) / 65_536.0),
        ),
        (
            "drinking_first_docked",
            first.map_or(0.0, |record| {
                f64::from(u32::from(record.measurement.docked))
            }),
        ),
        (
            "drinking_first_identity_matches",
            first.map_or(0.0, |record| f64::from(u32::from(record.identity_matches))),
        ),
        (
            "drinking_first_finishes",
            first.map_or(0.0, |record| f64::from(record.finishes)),
        ),
    ] {
        facts.insert(name.to_owned(), value);
    }
}

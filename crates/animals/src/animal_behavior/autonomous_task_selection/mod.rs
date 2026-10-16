//! Starts authored animal tasks through the existing behavior executor.

pub(super) mod candidate_facts;
mod scoring;
pub(super) mod task_cooldowns;
mod world_object_targets;

use super::{
    behavior_candidate_selection::{
        candidate_requirements_are_satisfied, BehaviorCandidateRanking,
    },
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_set_start_request_types::PendingBehaviorSet,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Frozen;
use openzt2_zoo_state::animal_health::types::Tranquilized;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use arrayvec::ArrayVec;
use bevy::prelude::*;
use candidate_facts::{AnimalCandidateFacts, AnimalCandidateFactsItem};
use openzt2_game_data::{
    behavior::{document::BehaviorTask, eligibility::BehaviorCandidateRole},
    AssetId,
};
use task_cooldowns::{AnimalTaskCooldowns, RunningDelayedAnimalTask};
use world_object_targets::{
    indexed_world_object_target_requirement, WorldObjectTargetLocation, WorldObjectTargetSources,
};
use super::behavior_token_execution::{BehaviorSelfToken, ClaimedBehaviorSelfToken};

pub(super) fn select_idle_animal_tasks(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    clock: Res<ZooClock>,
    mut execution_sequence: ResMut<BehaviorTaskExecutionSequence>,
    mut animals: Query<
        (
            Entity,
            AnimalCandidateFacts,
            &mut BehaviorRandomStream,
            Option<&mut AnimalTaskCooldowns>,
        ),
        (
            With<Animal>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<Destination>,
            Without<
                openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant,
            >,
        ),
    >,
    mut world_objects: WorldObjectTargetSources,
    tokens: Query<(Entity, &BehaviorSelfToken, &ChildOf), Without<ClaimedBehaviorSelfToken>>,
    mut commands: Commands,
) {
    world_objects.note_object_changes();
    if animals.is_empty() {
        return;
    }
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let Some(policy) = definitions.behavior_selection_policy() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    world_objects.refresh_index(definitions);
    // The scratch list returns to its owner below, keeping its allocation.
    let mut candidates = std::mem::take(&mut *world_objects.candidates);
    for (animal, facts, mut random, mut cooldowns) in &mut animals {
        let Some((definition, variant)) = species.find_variant_with_species(facts.variant_id())
        else {
            continue;
        };
        if definition.id != facts.species.species {
            continue;
        }
        let authored_types = &variant.behavior_subject_type_identifiers;
        // Actors which do not generate traversability have no area restriction.
        // Others reach only targets in the habitat they stand in.
        let area_restricted = authored_types
            .contains(&AssetId::from_key("b_generatestraversabilityinfo"))
            || authored_types.contains(&AssetId::from_key("unspecified"));
        let animal_habitat = world_objects.habitat_at(facts.transform.translation());
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let mut considered_candidates = 0_u32;
        for (document, declaration, task) in view.behavior_tasks() {
            if !subject_can_consider_task(task, &facts, authored_types) {
                continue;
            }
            if cooldowns
                .as_deref()
                .is_some_and(|cooldowns| cooldowns.is_cooling_down(task.id, clock.tick))
            {
                debug!(?animal, task = %task.name, id = ?task.id, "animal task cooling down");
                continue;
            }
            if task.targets.len() == 1 && task.targets[0] == "self" {
                let object_candidates = std::iter::once(None)
                    .filter(|_| task.objects.is_empty())
                    .chain(
                        (!task.objects.is_empty())
                            .then_some(())
                            .into_iter()
                            .flat_map(|()| tokens.iter())
                            .filter_map(|(entity, token, recipient)| {
                                (recipient.parent() == animal && clock.tick < token.expires_at)
                                    .then_some(Some((entity, token)))
                            }),
                    );
                for object in object_candidates {
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => {
                                facts.matches_type(required, authored_types)
                            }
                            BehaviorCandidateRole::Target => required == AssetId::from_key("self"),
                            BehaviorCandidateRole::Object => {
                                object.is_some_and(|(_, token)| token.name == required)
                            }
                        },
                        |role, fact| role != BehaviorCandidateRole::Object && facts.satisfies(fact),
                    ) {
                        trace!(?animal, task = %task.name, "animal self task qualifiers refused");
                        continue;
                    }
                    let Some(score) = scoring::score_animal_task(
                        task,
                        &facts,
                        policy.outside_range_need_value,
                        0.0,
                        &mut random,
                    ) else {
                        trace!(?animal, task = %task.name, "animal self task has no supported score");
                        continue;
                    };
                    considered_candidates += 1;
                    trace!(?animal, task = %task.name, score, priority = task.priority.unwrap_or_default(),
                    "animal self task scored");
                    ranking.consider(
                        (
                            document,
                            declaration,
                            task,
                            animal,
                            object.map(|(entity, _)| entity),
                        ),
                        task.priority.unwrap_or_default(),
                        score,
                        |upper| random.range_u32(upper),
                    );
                }
                continue;
            }
            // Token payloads and concrete object participants need their own
            // target producer; this path only considers an object-free target.
            if !task.objects.is_empty() {
                continue;
            }
            let Some(target_requirement) =
                indexed_world_object_target_requirement(&task.candidate_eligibility_requirements)
            else {
                trace!(?animal, task = %task.name, targets = ?task.targets,
                    "animal task targets are not supported world objects");
                continue;
            };
            world_objects
                .index
                .collect_candidates(target_requirement, &mut candidates);
            for candidate in candidates.iter().copied() {
                let Ok(target) = world_objects.objects.get(candidate) else {
                    debug!(?animal, task = %task.name, ?candidate,
                        "animal task candidate is crated or not a world object");
                    continue;
                };
                let Some(object) = definitions.find_object(target.definition.0) else {
                    continue;
                };
                if target.world.root != facts.world.root
                    || !target.has_consistent_container_state(object)
                {
                    debug!(?animal, task = %task.name, ?candidate,
                        same_world = target.world.root == facts.world.root,
                        "animal task candidate has another world or an unhydrated container");
                    continue;
                }
                let target_position = target.transform.translation();
                let location = WorldObjectTargetLocation {
                    in_habitat: !area_restricted
                        || animal_habitat.is_some_and(|habitat| {
                            world_objects.habitat_at(target_position) == Some(habitat)
                        }),
                    on_land: || world_objects.terrain_surfaces.on_land(target_position.xz()),
                };
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => {
                            facts.matches_type(required, authored_types)
                        }
                        BehaviorCandidateRole::Target => target.matches_type(required, object),
                        BehaviorCandidateRole::Object => false,
                    },
                    |role, fact| match role {
                        BehaviorCandidateRole::Subject => facts.satisfies(fact),
                        BehaviorCandidateRole::Target => target.satisfies(fact, &location),
                        BehaviorCandidateRole::Object => false,
                    },
                ) {
                    if bevy::log::tracing::enabled!(bevy::log::Level::DEBUG) {
                        let refused_target_facts = task
                            .candidate_eligibility_requirements
                            .iter()
                            .filter(|requirement| {
                                requirement.candidate_role == BehaviorCandidateRole::Target
                            })
                            .flat_map(|requirement| &requirement.eligibility_facts)
                            .filter(|fact| !target.satisfies(fact, &location))
                            .map(|fact| (fact.fact_input, fact.comparison, fact.expected_value))
                            .collect::<Vec<_>>();
                        let refused_subject_facts = task
                            .candidate_eligibility_requirements
                            .iter()
                            .filter(|requirement| {
                                requirement.candidate_role == BehaviorCandidateRole::Subject
                            })
                            .flat_map(|requirement| &requirement.eligibility_facts)
                            .filter(|fact| !facts.satisfies(fact))
                            .map(|fact| (fact.fact_input, fact.comparison, fact.expected_value))
                            .collect::<Vec<_>>();
                        debug!(?animal, task = %task.name, ?candidate,
                            in_habitat = location.in_habitat, area_restricted, ?animal_habitat,
                            target_habitat = ?world_objects.habitat_at(target_position),
                            ?refused_target_facts, ?refused_subject_facts,
                            "animal task target refused by qualifiers");
                    }
                    continue;
                }
                let distance = facts.transform.translation().distance(target_position);
                if !distance.is_finite() {
                    continue;
                }
                let Some(score) = scoring::score_animal_task(
                    task,
                    &facts,
                    policy.outside_range_need_value,
                    distance,
                    &mut random,
                ) else {
                    debug!(?animal, task = %task.name, ?candidate,
                        "animal task has no supported score");
                    continue;
                };
                considered_candidates += 1;
                debug!(?animal, task = %task.name, ?candidate, score, distance,
                    priority = task.priority.unwrap_or_default(),
                    minimum_score = policy.minimum_score,
                    "animal task target scored");
                ranking.consider(
                    (document, declaration, task, target.entity, None),
                    task.priority.unwrap_or_default(),
                    score,
                    |upper| random.range_u32(upper),
                );
            }
        }
        // The known candidate subset cannot establish a native singleton.
        // Do not promote a zero score merely because another producer is missing.
        let Some((document, declaration, task, target, object)) = ranking.finish(false) else {
            debug!(?animal, considered_candidates,
                thirst = ?facts.source_need(AssetId::from_key("thirst")),
                hunger = ?facts.source_need(AssetId::from_key("hunger")),
                outside_range_need_value = policy.outside_range_need_value,
                minimum_score = policy.minimum_score,
                "animal selected no task; needs are (deprivation q16, triggered)");
            continue;
        };
        let delay_ticks = task
            .task_delay_seconds
            .filter(|delay| *delay != [0.0, 0.0])
            .map(|delay| {
                task_cooldowns::sample_task_delay_ticks(
                    delay,
                    random.0.unit_f32(),
                    definitions.timing().fixed_hz,
                )
            });
        // Unimplemented actions must not promote a lower-ranked alternative.
        // A delayed task still waits out its delay, as though it had run.
        if task.execution.has_unsupported_suffix()
            || task.completion.has_unsupported_suffix()
            || task.failure.has_unsupported_suffix()
        {
            debug!(?animal, task = %task.name, ?target,
                execution = ?unsupported_action(&task.execution),
                completion = ?unsupported_action(&task.completion),
                failure = ?unsupported_action(&task.failure),
                cooldown_ticks = ?delay_ticks,
                "animal's chosen task has unsupported actions; no task starts");
            if let Some(delay_ticks) = delay_ticks {
                if let Some(cooldowns) = cooldowns.as_deref_mut() {
                    cooldowns.start(task.id, clock.tick, delay_ticks);
                } else {
                    let mut started = AnimalTaskCooldowns::default();
                    started.start(task.id, clock.tick, delay_ticks);
                    commands.entity(animal).insert(started);
                }
            }
            continue;
        }
        let execution_id = execution_sequence.next();
        if let Some(token) = object {
            commands
                .entity(token)
                .insert(ClaimedBehaviorSelfToken { execution_id });
        }
        debug!(?animal, task = %task.name, id = ?task.id, ?target, execution_id,
            cooldown_ticks = ?delay_ticks, "animal started task");
        let mut started = commands.entity(animal);
        started.insert(BehaviorTaskExecutionState {
            execution_id,
            program: task.id,
            target: Some(target),
            document: document.clone(),
            declaration,
            phase: BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: clock.tick,
            object,
            interaction_slot: None,
            stack: ArrayVec::new(),
        });
        match delay_ticks {
            Some(delay_ticks) => started.insert(RunningDelayedAnimalTask {
                execution_id,
                task: task.id,
                delay_ticks,
            }),
            None => started.remove::<RunningDelayedAnimalTask>(),
        };
    }
    *world_objects.candidates = candidates;
}

fn unsupported_action(
    phase: &openzt2_game_data::behavior::document::BehaviorTaskActionPhase,
) -> Option<&str> {
    use openzt2_game_data::behavior::document::BehaviorTaskActionPhase;
    match phase {
        BehaviorTaskActionPhase::Supported(_) => None,
        BehaviorTaskActionPhase::SupportedPrefix {
            first_unsupported_action,
            ..
        }
        | BehaviorTaskActionPhase::Unsupported {
            first_unsupported_action,
        } => Some(first_unsupported_action),
    }
}

fn subject_can_consider_task(
    task: &BehaviorTask,
    facts: &AnimalCandidateFactsItem<'_, '_>,
    authored_types: &[AssetId],
) -> bool {
    if task.targets.is_empty() || task.subjects.is_empty() {
        return false;
    }
    if !task
        .candidate_eligibility_requirements
        .iter()
        .any(|requirements| {
            requirements.candidate_role == BehaviorCandidateRole::Subject
                && !requirements.candidate_type_identifiers.is_empty()
        })
    {
        return false;
    }
    // Prune subject mismatches before scanning world targets, while retaining
    // all authored target checks for concrete candidate evaluation.
    candidate_requirements_are_satisfied(
        &task.candidate_eligibility_requirements,
        |role, required| {
            role != BehaviorCandidateRole::Subject || facts.matches_type(required, authored_types)
        },
        |role, fact| role != BehaviorCandidateRole::Subject || facts.satisfies(fact),
    )
}

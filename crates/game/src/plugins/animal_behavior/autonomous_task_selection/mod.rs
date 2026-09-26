//! Starts authored animal tasks through the existing behavior executor.

mod candidate_facts;
mod consumable_targets;
mod scoring;

use super::{
    behavior_candidate_selection::{
        candidate_requirements_are_satisfied, BehaviorCandidateRanking,
    },
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_set_start_request_types::PendingBehaviorSet,
};
use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::Dead;
use crate::plugins::animal_health::types::Frozen;
use crate::plugins::animal_health::types::Tranquilized;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::feeding::container_quantity::DrinkContainer;
use crate::plugins::feeding::container_quantity::FoodContainer;
use crate::plugins::locomotion::locomotion_types::Destination;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::world_entity_crating::WorldEntityIsCrated;
use arrayvec::ArrayVec;
use bevy::prelude::*;
use candidate_facts::{AnimalCandidateFacts, AnimalCandidateFactsItem};
use consumable_targets::{can_evaluate_consumable_fact, ConsumableCandidateFacts};
use openzt2_game_data::{
    behavior::{
        document::BehaviorTask,
        eligibility::{BehaviorCandidateRole, BehaviorEligibilityFactJunction},
    },
    AssetId,
};

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
        (Entity, AnimalCandidateFacts, &mut BehaviorRandomStream),
        (
            With<Animal>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<Destination>,
            Without<super::behavior_synchronized_set_execution::SynchronizedBehaviorParticipant>,
        ),
    >,
    consumables: Query<
        ConsumableCandidateFacts,
        (
            Or<(With<FoodContainer>, With<DrinkContainer>)>,
            Without<WorldEntityIsCrated>,
        ),
    >,
    mut commands: Commands,
) {
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
    for (animal, facts, mut random) in &mut animals {
        let Some(variant) = species.find_variant(facts.variant_id()) else {
            continue;
        };
        let authored_types = &variant.behavior_subject_type_identifiers;
        // Actors which do not generate traversability have no area restriction.
        // A requested but unavailable area is not equivalent to that state.
        let habitat_membership = (!authored_types
            .contains(&AssetId::from_key("b_generatestraversabilityinfo"))
            && !authored_types.contains(&AssetId::from_key("unspecified")))
        .then_some(true);
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        for (document, declaration, task) in view.behavior_tasks() {
            if !subject_can_consider_task(task, &facts, authored_types) {
                continue;
            }
            if task.targets.len() == 1 && task.targets[0] == "self" {
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => {
                            facts.matches_type(required, authored_types)
                        }
                        BehaviorCandidateRole::Target => required == AssetId::from_key("self"),
                        BehaviorCandidateRole::Object => false,
                    },
                    |role, fact| role != BehaviorCandidateRole::Object && facts.satisfies(fact),
                ) {
                    continue;
                }
                let Some(score) = scoring::score_animal_task(
                    task,
                    &facts,
                    policy.outside_range_need_value,
                    0.0,
                    &mut random,
                ) else {
                    continue;
                };
                ranking.consider(
                    (document, declaration, task, animal),
                    task.priority.unwrap_or_default(),
                    score,
                    |upper| random.range_u32(upper),
                );
                continue;
            }
            if !target_requirements_have_supported_facts(task, habitat_membership) {
                continue;
            }
            for target in &consumables {
                if target.world.root != facts.world.root
                    || target.source_quantity().is_none()
                    || definitions.find_object(target.definition.0).is_none()
                {
                    continue;
                }
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => {
                            facts.matches_type(required, authored_types)
                        }
                        BehaviorCandidateRole::Target => target.matches_type(required),
                        BehaviorCandidateRole::Object => false,
                    },
                    |role, fact| match role {
                        BehaviorCandidateRole::Subject => facts.satisfies(fact),
                        BehaviorCandidateRole::Target => target.satisfies(fact, habitat_membership),
                        BehaviorCandidateRole::Object => false,
                    },
                ) {
                    continue;
                }
                let distance = facts
                    .transform
                    .translation()
                    .distance(target.transform.translation());
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
                    continue;
                };
                ranking.consider(
                    (document, declaration, task, target.entity),
                    task.priority.unwrap_or_default(),
                    score,
                    |upper| random.range_u32(upper),
                );
            }
        }
        // The known candidate subset cannot establish a native singleton.
        // Do not promote a zero score merely because another producer is missing.
        if let Some((document, declaration, task, target)) = ranking.finish(false) {
            // Unimplemented actions must not promote a lower-ranked alternative.
            if task.execution.has_unsupported_suffix()
                || task.completion.has_unsupported_suffix()
                || task.failure.has_unsupported_suffix()
            {
                continue;
            }
            commands.entity(animal).insert(BehaviorTaskExecutionState {
                execution_id: execution_sequence.next(),
                program: task.id,
                target: Some(target),
                document: document.clone(),
                declaration,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: clock.tick,
                interaction_slot: None,
                stack: ArrayVec::new(),
            });
        }
    }
}

fn subject_can_consider_task(
    task: &BehaviorTask,
    facts: &AnimalCandidateFactsItem<'_, '_>,
    authored_types: &[AssetId],
) -> bool {
    if task
        .task_delay_seconds
        .is_some_and(|delay| delay != [0.0, 0.0])
        || !task.objects.is_empty()
        || task.targets.is_empty()
        || task.subjects.is_empty()
    {
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

fn target_requirements_have_supported_facts(
    task: &BehaviorTask,
    habitat_membership: Option<bool>,
) -> bool {
    let mut has_target = false;
    for requirements in &task.candidate_eligibility_requirements {
        if requirements.candidate_role != BehaviorCandidateRole::Target {
            continue;
        }
        has_target = true;
        if requirements.eligibility_facts.is_empty() {
            continue;
        }
        let supported = match requirements.eligibility_fact_junction {
            BehaviorEligibilityFactJunction::All => requirements
                .eligibility_facts
                .iter()
                .all(|fact| can_evaluate_consumable_fact(fact, habitat_membership)),
            BehaviorEligibilityFactJunction::Any => requirements
                .eligibility_facts
                .iter()
                .any(|fact| can_evaluate_consumable_fact(fact, habitat_membership)),
        };
        if !supported {
            return false;
        }
    }
    has_target
}

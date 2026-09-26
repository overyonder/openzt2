//! Bounded entry into the authored task executor for idle guests.

use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::{behavior::eligibility::BehaviorCandidateRole, AssetId};

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_behavior::behavior_candidate_selection::candidate_requirements_are_satisfied;
use crate::plugins::animal_behavior::behavior_candidate_selection::score_supported_behavior_task;
use crate::plugins::animal_behavior::behavior_candidate_selection::BehaviorCandidateRanking;
use crate::plugins::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use crate::plugins::animal_behavior::behavior_synchronized_set_execution::SynchronizedBehaviorParticipant;
use crate::plugins::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use crate::plugins::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::economy::price_effect::BehaviorPriceEffectContext;
use crate::plugins::economy::service_types::ServiceReservation;
use crate::plugins::locomotion::locomotion_types::Destination;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    guest_target_eligibility::satisfies_guest_target_fact,
    guest_task_need_facts::{GuestNeedTriggers, GuestTaskNeedFacts},
};

use super::guest_simulation_types::{
    Guest, GuestArchetype, GuestDestination, GuestPhase, GuestRng, Viewing,
};

pub(super) fn select_idle_guest_tasks(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    price_effect: BehaviorPriceEffectContext,
    mut executions: ResMut<BehaviorTaskExecutionSequence>,
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            &GuestPhase,
            &mut GuestRng,
            GuestTaskNeedFacts,
            &GuestNeedTriggers,
            &GlobalTransform,
            &WorldMember,
        ),
        (
            With<Guest>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<GuestDestination>,
            Without<Destination>,
            Without<ServiceReservation>,
            Without<Viewing>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
    targets: Query<(Entity, &DefinitionId, &GlobalTransform, &WorldMember)>,
    controllers: Query<
        &AnimationPresentationOwner,
        (
            With<AnimationPlaybackController>,
            With<AnimationGraphPlaybackState>,
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
    let Some(policy) = definitions.behavior_selection_policy() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    for (guest, archetype, phase, mut random, need_facts, triggers, transform, world) in &mut guests
    {
        if !transform.translation().is_finite()
            || *phase != GuestPhase::Visiting
            || !controllers
                .iter()
                .any(|owner| owner.gameplay_entity == guest)
        {
            continue;
        }
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        for (document, declaration, task) in view.behavior_tasks() {
            // Token and delayed candidates still
            // require their own source-backed producers and trigger lifetime.
            if !task.objects.is_empty()
                || task.targets.len() != 1
                || task.targets[0] == "fromToken"
                || task
                    .task_delay_seconds
                    .is_some_and(|delay| delay != [0.0, 0.0])
                || !task
                    .candidate_eligibility_requirements
                    .iter()
                    .any(|requirements| {
                        requirements.candidate_role == BehaviorCandidateRole::Subject
                            && !requirements.candidate_type_identifiers.is_empty()
                    })
            {
                continue;
            }
            // Reject unrelated subjects before scanning concrete world targets.
            if !candidate_requirements_are_satisfied(
                &task.candidate_eligibility_requirements,
                |role, required| {
                    role != BehaviorCandidateRole::Subject
                        || definition
                            .behavior_subject_type_identifiers
                            .contains(&required)
                },
                |role, fact| {
                    role != BehaviorCandidateRole::Subject || need_facts.satisfies(fact, triggers)
                },
            ) {
                continue;
            }
            let mut consider_target =
                |target: Entity, target_definition: Option<AssetId>, target_position: Vec3| {
                    let is_self = target == guest;
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => definition
                                .behavior_subject_type_identifiers
                                .contains(&required),
                            BehaviorCandidateRole::Target => {
                                if is_self {
                                    required == AssetId::from_key("self")
                                } else {
                                    target_definition == Some(required)
                                }
                            }
                            BehaviorCandidateRole::Object => false,
                        },
                        |role, fact| {
                            if role == BehaviorCandidateRole::Object {
                                return false;
                            }
                            if role == BehaviorCandidateRole::Target && !is_self {
                                return satisfies_guest_target_fact(
                                    fact,
                                    transform.translation(),
                                    target_position,
                                );
                            }
                            need_facts.satisfies(fact, triggers)
                        },
                    ) {
                        return;
                    }
                    let Some(score) = score_supported_behavior_task(
                        task,
                        policy.outside_range_need_value,
                        transform.translation().distance(target_position),
                        |attribute| need_facts.source_need(attribute, triggers),
                        |value| {
                            price_effect.sample(value, Some(target), |upper| {
                                random.0.range_u32(upper).unwrap_or_default()
                            })
                        },
                    ) else {
                        return;
                    };
                    ranking.consider(
                        (document, declaration, task, target),
                        task.priority.unwrap_or_default(),
                        score,
                        |upper| random.0.range_u32(upper),
                    );
                };
            if task.targets[0] == "self" {
                consider_target(guest, None, transform.translation());
            } else {
                for (target, target_definition, target_transform, target_world) in &targets {
                    if target == guest
                        || target_world.root != world.root
                        || definitions.find_object(target_definition.0).is_none()
                        || !target_transform.translation().is_finite()
                    {
                        continue;
                    }
                    consider_target(
                        target,
                        Some(target_definition.0),
                        target_transform.translation(),
                    );
                }
            }
        }
        // The supported candidate subset cannot prove native singleton status.
        let Some((document, declaration, task, target)) = ranking.finish(false) else {
            continue;
        };
        // An unsupported winning action does not promote a lower-ranked choice.
        if task.execution.has_unsupported_suffix()
            || task.completion.has_unsupported_suffix()
            || task.failure.has_unsupported_suffix()
        {
            continue;
        }
        commands.entity(guest).insert(BehaviorTaskExecutionState {
            execution_id: executions.next(),
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

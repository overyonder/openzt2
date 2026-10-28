//! Bounded entry into the authored task executor for idle guests.

use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::{
    behavior::eligibility::BehaviorCandidateRole,
    world_definitions::world_objects::WorldObjectDefinition, AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::candidate_requirements_are_satisfied;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::score_supported_behavior_task;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorCandidateRanking;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::price_effect::BehaviorPriceEffectContext;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    guest_target_eligibility::satisfies_guest_target_fact,
    guest_task_need_facts::{GuestNeedTriggers, GuestTaskNeedFacts},
};

use super::guest_simulation_types::{
    Guest, GuestArchetype, GuestDestination, GuestPhase, GuestRng, Viewing,
};

/// Authored `TaskDelayMin`/`TaskDelayMax`: once a task ends, it stays
/// unavailable to the same guest for a delay sampled when it was chosen.
#[derive(Component, Default)]
pub(super) struct GuestTaskCooldowns {
    /// The chosen delayed task and its sampled delay, until the guest is idle again.
    running: Option<(AssetId, u64)>,
    /// Delayed tasks and the tick from which each may be chosen again.
    ready_ticks: Vec<(AssetId, u64)>,
}

impl GuestTaskCooldowns {
    /// Starts the cooldown of a delayed task that has ended and forgets expired ones.
    fn observe_idle(&mut self, tick: u64) {
        if let Some((task, delay_ticks)) = self.running.take() {
            self.ready_ticks.retain(|(cooling, _)| *cooling != task);
            self.ready_ticks
                .push((task, tick.saturating_add(delay_ticks)));
        }
        self.ready_ticks.retain(|(_, ready)| *ready > tick);
    }

    fn is_cooling(&self, task: AssetId) -> bool {
        self.ready_ticks.iter().any(|(cooling, _)| *cooling == task)
    }
}

pub(super) fn select_idle_guest_tasks(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    fixed_time: Res<Time<Fixed>>,
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
            Option<&mut GuestTaskCooldowns>,
        ),
        (
            With<Guest>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<GuestDestination>,
            Without<Destination>,
            Without<ServiceReservation>,
            Without<Viewing>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
    targets: Query<(Entity, &DefinitionId, &GlobalTransform, &WorldMember)>,
    held: Query<&DefinitionId>,
    occupancy: Res<InteractionContainerOccupancy>,
    paths: Query<(Entity, &GlobalTransform, &WorldMember, &PathTile)>,
    viewable_animals: Query<
        (&GlobalTransform, &Visibility, &WorldMember),
        (With<Animal>, With<HabitatMember>),
    >,
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
    if guests.is_empty() {
        return;
    }
    // The original offers viewing tasks the paths its `viewanimals` sensor
    // finds: path tiles with an animal in its habitat within viewing range.
    let view_sensor = AssetId::from_key("viewanimals");
    let viewing_paths = definitions
        .guest_viewing()
        .map(|viewing| {
            let range_squared = viewing.ground_range_m * viewing.ground_range_m;
            paths
                .iter()
                .filter(|(_, path, path_world, _)| {
                    viewable_animals
                        .iter()
                        .any(|(animal, visibility, animal_world)| {
                            animal_world.root == path_world.root
                                && *visibility != Visibility::Hidden
                                && animal
                                    .translation()
                                    .xz()
                                    .distance_squared(path.translation().xz())
                                    <= range_squared
                        })
                })
                .map(|(path, transform, path_world, tile)| {
                    let elevated = definitions
                        .find_path(tile.definition)
                        .is_some_and(|definition| definition.elevated);
                    (path, transform.translation(), path_world.root, elevated)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (
        guest,
        archetype,
        phase,
        mut random,
        need_facts,
        triggers,
        transform,
        world,
        mut cooldowns,
    ) in &mut guests
    {
        if let Some(cooldowns) = cooldowns.as_deref_mut() {
            cooldowns.observe_idle(clock.tick);
        }
        // Gate refusals are reported by the per-guest task progress diagnostics.
        if !transform.translation().is_finite()
            || *phase != GuestPhase::Visiting
            || !controllers
                .iter()
                .any(|owner| owner.gameplay_entity == guest)
        {
            continue;
        }
        let Some(definition) = definitions.find_guest(archetype.0) else {
            debug!(?guest, archetype = ?archetype.0, "idle guest has no guest definition");
            continue;
        };
        // Items the guest holds, such as a bought burger, for tasks that use one.
        let held_items = occupancy
            .members_of_container(guest)
            .filter_map(|item| {
                let object = definitions.find_object(held.get(item).ok()?.0)?;
                Some((item, object))
            })
            .collect::<Vec<_>>();
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let diagnostics = bevy::log::tracing::enabled!(bevy::log::Level::DEBUG);
        for (document, declaration, task) in view.behavior_tasks() {
            // Token candidates still require their own source-backed producers.
            if task.targets.len() != 1
                || task.targets[0] == "fromtoken"
                || cooldowns
                    .as_deref()
                    .is_some_and(|cooldowns| cooldowns.is_cooling(task.id))
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
                    role != BehaviorCandidateRole::Subject
                        || need_facts.satisfies(
                            fact,
                            triggers,
                            &definition.behavior_subject_type_identifiers,
                        )
                },
            ) {
                continue;
            }
            let matches_type = |object: &WorldObjectDefinition, required: AssetId| {
                object.id == required || object.behavior_type_identifiers.contains(&required)
            };
            // A task that uses an item takes the first held item its `Objects` name.
            let object = if task.objects.is_empty() {
                None
            } else {
                let Some(&(item, object)) = held_items.iter().find(|(_, object)| {
                    candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| {
                            role != BehaviorCandidateRole::Object || matches_type(object, required)
                        },
                        |role, _| role != BehaviorCandidateRole::Object,
                    )
                }) else {
                    continue;
                };
                Some((item, object))
            };
            let mut consider_target =
                |target: Entity,
                 target_definition: Option<&WorldObjectDefinition>,
                 elevated_path: Option<bool>,
                 target_position: Vec3| {
                    let is_path = elevated_path.is_some();
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
                                } else if is_path {
                                    required == AssetId::from_key("path")
                                } else {
                                    target_definition
                                        .is_some_and(|object| matches_type(object, required))
                                }
                            }
                            BehaviorCandidateRole::Object => {
                                object.is_some_and(|(_, object)| matches_type(object, required))
                            }
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
                                    elevated_path,
                                );
                            }
                            need_facts.satisfies(
                                fact,
                                triggers,
                                &definition.behavior_subject_type_identifiers,
                            )
                        },
                    ) {
                        // Report world objects of a named type whose qualifiers refused them.
                        if diagnostics
                            && target_definition.is_some()
                            && candidate_requirements_are_satisfied(
                                &task.candidate_eligibility_requirements,
                                |role, required| match role {
                                    BehaviorCandidateRole::Target => target_definition
                                        .is_some_and(|object| matches_type(object, required)),
                                    _ => true,
                                },
                                |_, _| true,
                            )
                        {
                            debug!(?guest, ?target, task = %task.name,
                                distance_m = transform.translation().xz().distance(target_position.xz()),
                                "guest task target refused by its authored qualifiers");
                        }
                        return;
                    }
                    let score = score_supported_behavior_task(
                        task,
                        policy.outside_range_need_value,
                        transform.translation().distance(target_position),
                        |attribute| need_facts.source_need(attribute, triggers),
                        |value| {
                            price_effect.sample(value, Some(target), |upper| {
                                random.0.range_u32(upper).unwrap_or_default()
                            })
                        },
                    );
                    if diagnostics && target_definition.is_some() {
                        let needs = task
                            .scores
                            .iter()
                            .filter_map(|entry| {
                                match entry {
                                openzt2_game_data::behavior::score::BehaviorScore::AttributeValue {
                                    attribute,
                                    ..
                                } => Some(need_facts.source_need(*attribute, triggers)),
                                _ => None,
                            }
                            })
                            .collect::<Vec<_>>();
                        debug!(?guest, ?target, task = %task.name, ?score,
                            minimum_score = policy.minimum_score,
                            needs_q16_and_triggered = ?needs,
                            "guest task target scored");
                    }
                    let Some(score) = score else {
                        return;
                    };
                    ranking.consider(
                        (
                            document,
                            declaration,
                            task,
                            target,
                            object.map(|(item, _)| item),
                        ),
                        task.priority.unwrap_or_default(),
                        score,
                        |upper| random.0.range_u32(upper),
                    );
                };
            if task.targets[0] == "self" {
                consider_target(guest, None, None, transform.translation());
            } else if task.targets[0] == "path" {
                // Other path sensors, such as litter disposal, are not modelled.
                if task.sensor_tag == Some(view_sensor) {
                    for &(path, position, root, elevated) in &viewing_paths {
                        if root == world.root {
                            consider_target(path, None, Some(elevated), position);
                        }
                    }
                }
            } else {
                for (target, target_definition, target_transform, target_world) in &targets {
                    if target == guest
                        || target_world.root != world.root
                        || !target_transform.translation().is_finite()
                    {
                        continue;
                    }
                    let Some(object) = definitions.find_object(target_definition.0) else {
                        continue;
                    };
                    consider_target(target, Some(object), None, target_transform.translation());
                }
            }
        }
        // The supported candidate subset cannot prove native singleton status.
        let Some(((document, declaration, task, target, object), score)) = ranking.finish(false)
        else {
            debug!(?guest, "idle guest found no task scoring above the minimum");
            continue;
        };
        // The authored delay starts once the task ends. An unsupported winner
        // ends at once, so its delay still lets lower-ranked tasks run meanwhile.
        if let Some([minimum, maximum]) =
            task.task_delay_seconds.filter(|delay| *delay != [0.0, 0.0])
        {
            let seconds = minimum + (maximum - minimum).max(0.0) * random.0.unit_f32();
            let delay_ticks =
                (f64::from(seconds) / fixed_time.timestep().as_secs_f64()).ceil() as u64;
            if let Some(cooldowns) = cooldowns.as_deref_mut() {
                cooldowns.running = Some((task.id, delay_ticks));
            } else {
                commands.entity(guest).insert(GuestTaskCooldowns {
                    running: Some((task.id, delay_ticks)),
                    ready_ticks: Vec::new(),
                });
            }
        }
        // An unsupported winning action does not promote a lower-ranked choice.
        if task.execution.has_unsupported_suffix()
            || task.completion.has_unsupported_suffix()
            || task.failure.has_unsupported_suffix()
        {
            debug!(?guest, task = %task.name, "idle guest's best task has unsupported actions");
            continue;
        }
        debug!(?guest, task = %task.name, ?target, self_target = target == guest,
            "idle guest chose a task");
        let execution_id = executions.next();
        commands.entity(guest).insert(BehaviorTaskEffectiveScore {
            execution_id,
            template_score: Some(task.priority.unwrap_or_default()),
            linked_score: None,
            selected_candidate_score: Some(score),
        });
        commands.entity(guest).insert(BehaviorTaskExecutionState {
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
    }
}

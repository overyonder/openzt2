//! Authored animal viewing through the current viewpoint and its live subjects.
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorViewingProgress;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorTaskReplaced, RunBehaviorViewAnimals, RunningBehaviorInstruction, SelectedBehaviorSet,
};

use crate::donations::authored_view_donation_tokens::offer_view_donations;
use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::EducationDonationBonus,
    donations::donation_guest_decision_types::{
        GuestDonationMeters, SpeciesDonationCandidateLifetimes,
    },
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::GuestSatisfaction,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_simulation_types::{Guest, GuestArchetype, GuestPhase, Viewing};

pub(super) fn retire_finished_view_meter(
    removed: On<Remove<BehaviorTaskExecutionState>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<BehaviorViewingProgress>();
}

pub(super) fn retire_replaced_view_meter(
    replaced: On<BehaviorTaskReplaced>,
    progress: Query<&BehaviorViewingProgress>,
    mut commands: Commands,
) {
    if progress
        .get(replaced.actor)
        .is_ok_and(|meter| meter.execution == replaced.previous_execution)
    {
        commands
            .entity(replaced.actor)
            .try_remove::<BehaviorViewingProgress>();
    }
}

/// Starts, holds and ends observation for the running view action.
/// The observation's area is the task's target, the path viewed from. Its
/// emote sets look at and affect the viewed animal, so they target it.
pub(super) fn watch_animals_for_current_view_actions(
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    time: Res<Time<Fixed>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    behaviors: Res<Assets<BehaviorDocumentAsset>>,
    active_species: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    mut random: ResMut<
        openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream,
    >,
    mut guests: Query<
        (
            Entity,
            (
                &mut BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorViewAnimals,
            ),
            &GlobalTransform,
            &WorldMember,
            &GuestPhase,
            Option<&Viewing>,
            &GuestArchetype,
            Option<&mut BehaviorViewingProgress>,
            Option<(
                &mut GuestDonationMeters,
                &mut SpeciesDonationCandidateLifetimes,
                &Wallet,
                &GuestSatisfaction,
                Has<EducationDonationBonus>,
            )>,
        ),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    animals: Query<
        (Entity, &GlobalTransform, &Visibility, &WorldMember),
        (
            With<Animal>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity,
        ),
    >,
    viewpoints: Query<(
        Entity,
        &GlobalTransform,
        &WorldMember,
        &super::guest_viewing_area_sensor::GuestViewpointSupports,
    )>,
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    let Some(range_m) = definitions
        .guest_viewing()
        .map(|viewing| viewing.ground_range_m)
    else {
        return;
    };
    for (
        guest,
        (mut task, instruction, running_action),
        transform,
        world,
        phase,
        viewing,
        archetype,
        meter,
        donation,
    ) in behavior_action_actors(&mut guests, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(&task) {
            continue;
        }
        let action = &running_action.0;
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let mut meter = meter.filter(|meter| {
            meter.execution == task.execution_id && task.is_at_exact_frame(&meter.at, meter.depth)
        });
        if let Some(meter) = meter.as_deref_mut() {
            if meter.instruction != instruction.id() {
                meter.instruction = instruction.id();
            }
        }
        let repetitions = task.repetitions;
        let set = |program, target| {
            let index = documents.create_declaration_index_view(&behaviors)?;
            let (document, declaration) = index.find_behavior_set_location(
                program,
                definition.behavior_subject_type_identifiers.iter().copied(),
            )?;
            Some(SelectedBehaviorSet {
                program,
                document,
                declaration,
                tick: clock.tick,
                repeat: Some(repetitions),
                target,
            })
        };
        let origin = transform.translation().xz();
        let viewpoint = task.target();
        let supports = viewpoint
            .and_then(|point| viewpoints.get(point).ok())
            .map(|(_, _, _, supports)| supports.0.as_slice())
            .unwrap_or_default();
        let in_view = |animal: Entity| {
            supports.contains(&animal)
                && animals
                    .get(animal)
                    .is_ok_and(|(_, animal, visibility, animal_world)| {
                        animal_world.root == world.root
                            && *visibility != Visibility::Hidden
                            && animal.translation().xz().distance_squared(origin)
                                <= range_m * range_m
                    })
        };
        let viewing = viewing.filter(|viewing| {
            viewing.area == viewpoint && viewing.task_execution_id == Some(task.execution_id)
        });
        let assigned_subject = || {
            viewpoint
                .and_then(|point| viewpoints.get(point).ok())
                .and_then(|(_, _, _, supports)| {
                    supports.0.iter().copied().find(|&subject| {
                        in_view(subject) && viewing.is_none_or(|viewing| viewing.subject != subject)
                    })
                })
        };
        match viewing {
            None => {
                let (Some(subject), GuestPhase::Visiting) = (assigned_subject(), phase) else {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                    continue;
                };
                if let Some(meter) = meter.as_deref_mut() {
                    for _ in supports.iter().copied().filter(|&animal| in_view(animal)) {
                        meter.apply_view_score(1.0);
                    }
                }
                commands.entity(guest).insert(Viewing {
                    subject,
                    area: viewpoint,
                    started_tick: clock.tick,
                    task_execution_id: Some(task.execution_id),
                });
            }
            Some(viewing) => {
                let watched = meter.as_deref_mut().is_some_and(|meter| {
                    let completed = meter.remaining < 0.0;
                    meter.remaining -= time.delta_secs() * definition.view_meter_decay_per_second;
                    completed
                });
                let subject_visible = in_view(viewing.subject);
                if !watched
                    && subject_visible
                    && meter
                        .as_ref()
                        .is_some_and(|meter| meter.emote_score.is_some())
                {
                    let target = animals
                        .get(viewing.subject)
                        .ok()
                        .map(|(_, pose, ..)| pose.translation().xz());
                    let policy = definition.view_scoring;
                    if let Some((dock_set, chance, target)) = action
                        .dock_viewpoint_behavior_set
                        .zip(policy.viewpoint_swap_chance)
                        .zip(target)
                        .map(|((set, chance), target)| (set, chance, target))
                    {
                        if f32::from(random.next_u15() % 100) < chance {
                            let [min, max] = policy.viewpoint_swap_distance;
                            let current_distance = origin.distance_squared(target);
                            let next = viewpoints
                                .iter()
                                .filter_map(|(path, pose, member, supports)| {
                                    let point = pose.translation().xz();
                                    let from_here = point.distance_squared(origin);
                                    let from_subject = point.distance_squared(target);
                                    (member.root == world.root
                                        && Some(path) != task.target()
                                        && supports.0.contains(&viewing.subject)
                                        && min * min < from_here
                                        && from_here < max * max
                                        && from_subject < current_distance
                                        && from_subject <= range_m * range_m)
                                        .then_some((path, from_subject))
                                })
                                .min_by(|lhs, rhs| {
                                    lhs.1.total_cmp(&rhs.1).then_with(|| lhs.0.cmp(&rhs.0))
                                });
                            if let Some((path, _)) = next {
                                if let Some(candidate) = set(dock_set, Some(Some(path))) {
                                    task.target = Some(path);
                                    task.interaction_slot = None;
                                    commands.entity(guest).remove::<Viewing>();
                                    instruction.enter_set(guest, candidate, &mut commands);
                                    continue;
                                }
                            }
                        }
                    }
                    if let Some(score) = meter
                        .as_deref_mut()
                        .and_then(|meter| meter.emote_score.take())
                    {
                        let emote = action.emote_sets.iter().find(|emote| {
                            let [minimum, maximum] = emote.view_meter_range;
                            minimum <= score && score < maximum
                        });
                        if let Some(selected) = emote
                            .and_then(|emote| set(emote.behavior_set, Some(Some(viewing.subject))))
                        {
                            instruction.enter_set(guest, selected, &mut commands);
                            continue;
                        }
                    }
                }
                if !watched && !subject_visible && action.retarget == Some(true) {
                    if let Some(subject) = assigned_subject() {
                        commands.entity(guest).insert(Viewing {
                            subject,
                            started_tick: clock.tick,
                            ..*viewing
                        });
                        continue;
                    }
                }
                if watched || !subject_visible {
                    if let (
                        true,
                        Some(policy),
                        Some((mut meters, mut candidates, wallet, happiness, bonus)),
                        Some(timestep),
                    ) = (
                        watched,
                        definition.view_scoring.education_donation,
                        donation,
                        definitions.simulation_timestep(),
                    ) {
                        offer_view_donations(
                            guest,
                            policy,
                            &mut meters,
                            &mut candidates,
                            active_species.get(&species_assets),
                            wallet,
                            happiness,
                            bonus,
                            clock.tick,
                            timestep,
                            &mut commands,
                        );
                    }
                    commands
                        .entity(guest)
                        .remove::<(Viewing, BehaviorViewingProgress)>();
                    instruction.complete(guest, clock.tick, &mut commands);
                    continue;
                }
            }
        }
        if meter.is_none() {
            let [minimum, maximum] = action.view_meter_start_range;
            let start = if minimum >= 0.0 && maximum >= minimum {
                let width = ((maximum - minimum) as u32).saturating_add(1);
                minimum + (u32::from(random.next_u15()) % width) as f32
            } else if action.view_meter_start > 0.0 {
                action.view_meter_start
            } else {
                60.0
            };
            commands.entity(guest).insert(BehaviorViewingProgress {
                instruction: instruction.id(),
                execution: task.execution_id,
                at: task.create_return_frame_after_action(task.action),
                depth: task.depth,
                remaining: start,
                maximum: start,
                emote_score: viewing.is_none().then_some(1.0),
            });
        }
    }
}

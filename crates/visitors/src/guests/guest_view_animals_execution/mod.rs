//! Authored animal viewing: a docked guest watches the nearest animal in view.
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorViewingProgress;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorTaskReplaced, RunBehaviorViewAnimals, RunningBehaviorInstruction, SelectedBehaviorSet,
};

use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_data::{
    AssetId, world_definitions::guest_simulation_definitions::GuestEducationDonationPolicy,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{
        BehaviorSelfToken, BehaviorTokenEconomyTerms, EducationDonationBonus,
    },
    donations::donation_guest_decision_types::GuestDonationMeters,
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::GuestSatisfaction,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_simulation_types::{Guest, GuestArchetype, GuestPhase, GuestRng, Viewing};

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

/// Starts, holds and ends the native observation for the running view action.
/// The observation's area is the task's target, the path viewed from.
pub(super) fn watch_animals_for_current_view_actions(
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    time: Res<Time<Fixed>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    behaviors: Res<Assets<BehaviorDocumentAsset>>,
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
            &mut GuestRng,
            Option<&mut BehaviorViewingProgress>,
            Option<(
                &mut GuestDonationMeters,
                &Wallet,
                &GuestSatisfaction,
                Has<EducationDonationBonus>,
            )>,
        ),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    animals: Query<
        (Entity, &GlobalTransform, &Visibility, &WorldMember),
        (With<Animal>, With<HabitatMember>),
    >,
    paths: Query<(Entity, &GlobalTransform, &WorldMember), With<PathTile>>,
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
        mut random,
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
        let in_view = |animal: Entity| {
            animals
                .get(animal)
                .is_ok_and(|(_, animal, visibility, animal_world)| {
                    animal_world.root == world.root
                        && *visibility != Visibility::Hidden
                        && animal.translation().xz().distance_squared(origin) <= range_m * range_m
                })
        };
        let viewing = viewing.filter(|viewing| {
            viewing.area == task.target() && viewing.task_execution_id == Some(task.execution_id)
        });
        let nearest_visible_animal = || {
            animals
                .iter()
                .filter(|(animal, ..)| in_view(*animal))
                .min_by(|(_, left, ..), (_, right, ..)| {
                    left.translation()
                        .xz()
                        .distance_squared(origin)
                        .total_cmp(&right.translation().xz().distance_squared(origin))
                })
                .map(|(animal, ..)| animal)
        };
        match viewing {
            None => {
                let (Some(subject), GuestPhase::Visiting) = (nearest_visible_animal(), phase)
                else {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                    continue;
                };
                commands.entity(guest).insert(Viewing {
                    subject,
                    area: task.target(),
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
                if !watched && subject_visible {
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
                        if ((random.0.next_u32() % 100) as f32) < chance {
                            let [min, max] = policy.viewpoint_swap_distance;
                            let current_distance = origin.distance_squared(target);
                            let next = paths
                                .iter()
                                .filter_map(|(path, pose, member)| {
                                    let point = pose.translation().xz();
                                    let from_here = point.distance_squared(origin);
                                    let from_subject = point.distance_squared(target);
                                    (member.root == world.root
                                        && Some(path) != task.target()
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
                        if let Some(selected) =
                            emote.and_then(|emote| set(emote.behavior_set, None))
                        {
                            instruction.enter_set(guest, selected, &mut commands);
                            continue;
                        }
                    }
                }
                if !watched && !subject_visible && action.retarget == Some(true) {
                    if let Some(subject) = nearest_visible_animal() {
                        commands.entity(guest).insert(Viewing {
                            subject,
                            started_tick: clock.tick,
                            ..*viewing
                        });
                        continue;
                    }
                }
                if watched || !subject_visible {
                    if let (Some(policy), Some((mut meters, wallet, happiness, bonus))) =
                        (definition.view_scoring.education_donation, donation)
                    {
                        issue_education_donation_token(
                            guest,
                            policy,
                            &mut meters,
                            wallet,
                            happiness,
                            bonus,
                            clock.tick,
                            definitions.timing().fixed_hz,
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
            let start = if minimum > 0.0 && maximum > minimum {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "positive authored view-meter ranges are sampled in whole units"
                )]
                let width = (maximum - minimum) as u32;
                let Some(upper) = width.checked_add(1) else {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                    continue;
                };
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "the authored meter uses float values for its whole-unit random samples"
                )]
                let offset = random.0.range_u32(upper).unwrap_or_default() as f32;
                minimum + offset
            } else {
                action.view_meter_start
            };
            commands.entity(guest).insert(BehaviorViewingProgress {
                instruction: instruction.id(),
                execution: task.execution_id,
                at: task.create_return_frame_after_action(task.action),
                depth: task.depth,
                remaining: start,
                maximum: start,
                emote_score: None,
            });
        }
    }
}

fn issue_education_donation_token(
    guest: Entity,
    policy: GuestEducationDonationPolicy,
    meters: &mut GuestDonationMeters,
    wallet: &Wallet,
    happiness: &GuestSatisfaction,
    bonus: bool,
    tick: u64,
    fixed_hz: u16,
    commands: &mut Commands,
) {
    let tracker = AssetId::from_key("donate_education");
    let Some(points) = meters.0.get_mut(&tracker) else {
        return;
    };
    let mut amount = *points / policy.divisor;
    if amount < policy.minimum {
        return;
    }
    if let Some(maximum) = policy.maximum {
        amount = amount.min(maximum);
    }
    let deprivation = happiness.source_q16() as f32 / 65_536.0;
    if policy
        .happiness_threshold
        .is_some_and(|threshold| deprivation > threshold)
    {
        amount *= (100.0 - deprivation) / 100.0;
    }
    if amount <= 0.0 {
        return;
    }
    *points = 0.0;
    if bonus {
        amount *= policy.bonus_factor;
        commands
            .entity(guest)
            .try_remove::<EducationDonationBonus>();
    }
    if f64::from(amount) * 100.0 > wallet.0.0 as f64 {
        return;
    }
    let seconds = policy.timeout.unwrap_or(60.0);
    let expires_at = if seconds < 0.0 {
        u64::MAX
    } else {
        tick.saturating_add((f64::from(seconds) * f64::from(fixed_hz)).ceil() as u64)
    };
    commands.spawn((
        ChildOf(guest),
        BehaviorSelfToken {
            name: AssetId::from_key("t_donateed"),
            available_at: tick,
            expires_at,
            payload: None,
            retain_on_failure: false,
        },
        BehaviorTokenEconomyTerms {
            cost_override: amount,
            tracker,
        },
    ));
}

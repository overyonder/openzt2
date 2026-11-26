use bevy::{prelude::*, transform::helper::TransformHelper};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorPursuit, RunningBehaviorInstruction,
};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::{AnimalCrate, ScriptedCrating};
use openzt2_zoo_state::animal_behavior::behavior_pursuit_types::{
    BehaviorPursuitLeg, BehaviorPursuitPoll, PendingBehaviorPursuit,
};
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_zoo_state::locomotion::locomotion_types::PursueEntity;
use openzt2_zoo_state::locomotion::locomotion_types::PursuitRefusalRoute;
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, EntityPursuit};
use openzt2_zoo_state::locomotion::NavigationRequestAcceptance;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

pub(super) fn register_pursuit_execution(application: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    application.add_observer(
        activated_behavior_actor::<
            openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorPursuit,
        >
            .pipe(advance_current_pursuit_actions),
    );

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    application.add_systems(
        FixedUpdate,
        (
            record_pursuit_navigation_results,
            fail_pursuits_of_unavailable_targets,
            update_waiting_behavior_actors
                .pipe(advance_current_pursuit_actions)
                .after(record_pursuit_navigation_results)
                .after(fail_pursuits_of_unavailable_targets),
        )
            .in_set(BehaviorStepFixedSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
    application.add_systems(
        FixedUpdate,
        settle_pursuits_after_navigation_acceptance
            .after(NavigationRequestAcceptance)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
    application.add_observer(discard_pursuit_on_task_removal);
    application.add_systems(
        FixedPostUpdate,
        complete_pursuit_hits_after_motion
            .after(avian3d::prelude::PhysicsSystems::Writeback)
            .run_if(in_state(GamePhase::InGame)),
    );
}

/// Applies locomotion's arrivals and failures to the legs the pursuit owns.
/// A refused direct line is answered by its route, so it is not a failure.
fn record_pursuit_navigation_results(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut pursuers: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &mut PendingBehaviorPursuit,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for failure in failures.read() {
        let Ok((task, instruction, pending)) = pursuers.get(failure.entity) else {
            continue;
        };
        if !pending.owns(task, instruction)
            || !pending
                .leg
                .navigation_request_ids()
                .contains(&Some(failure.request_id))
            || (failure.reason == NavigationFailure::NoRoute
                && pending.leg.is_direct_request(failure.request_id))
        {
            continue;
        }
        mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
    }
    for arrival in arrivals.read() {
        let Ok((task, instruction, mut pending)) = pursuers.get_mut(arrival.entity) else {
            continue;
        };
        if !pending.owns(task, instruction) {
            continue;
        }
        let arrived = arrival.request_id;
        pending.leg = match pending.leg {
            BehaviorPursuitLeg::Route { request_id }
            | BehaviorPursuitLeg::DirectRequested {
                route_request_id: request_id,
                ..
            }
            | BehaviorPursuitLeg::Direct {
                refused_route_request_id: Some(request_id),
                ..
            } if request_id == arrived => BehaviorPursuitLeg::RouteArrived,
            BehaviorPursuitLeg::DirectRequested { request_id, .. }
            | BehaviorPursuitLeg::Direct { request_id, .. }
                if request_id == arrived =>
            {
                BehaviorPursuitLeg::DirectArrived { request_id }
            }
            _ => continue,
        };
    }
}

/// A pursuit fails while its target is out of the world, crated or held for
/// relocation, or while the target's own running task outscores the
/// pursuer's.
#[allow(clippy::type_complexity)]
fn fail_pursuits_of_unavailable_targets(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    pursuers: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorPursuit,
            ),
            Option<&BehaviorTaskEffectiveScore>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    targets: Query<(
        (Has<AnimalCrate>, Has<ScriptedCrating>),
        Has<RelocatingPlacedObject>,
        Option<&BehaviorTaskExecutionState>,
        Option<&BehaviorTaskEffectiveScore>,
    )>,
) {
    for (pursuer, (task, instruction, _), score) in &pursuers {
        if task.next_action_tick > clock.tick || !instruction.is_current(task) {
            continue;
        }
        let Some(Ok((crated, relocating, target_task, target_score))) =
            task.target().map(|target| targets.get(target))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(pursuer, &mut commands);
            continue;
        };
        let outscored = target_task.is_some_and(|target_task| {
            let target = BehaviorTaskEffectiveScore::of_running_task(target_score, target_task);
            target > 0.0 && target > BehaviorTaskEffectiveScore::of_running_task(score, task)
        });
        if crated.0 || crated.1 || relocating || outscored {
            mark_behavior_task_for_failure_and_stop_navigation(pursuer, &mut commands);
        }
    }
}

/// One update of the authored pursuit: route toward the target, try direct
/// steering inside the pursuit radius, and poll the failure radius and the
/// direct line at the authored interval. Distances are horizontal.
#[allow(
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn advance_current_pursuit_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    time: Res<Time<Fixed>>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut route_requests: MessageWriter<NavigateTo>,
    mut pursuit_requests: MessageWriter<PursueEntity>,
    mut actors: Query<
        (
            Entity,
            (
                Option<&BehaviorTaskExecutionState>,
                Option<(&RunningBehaviorInstruction, &RunBehaviorPursuit)>,
            ),
            Has<NavAgent>,
            Option<&mut PendingBehaviorPursuit>,
            Option<&mut BehaviorPursuitPoll>,
            Option<&Destination>,
            Option<&EntityPursuit>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(With<RunBehaviorPursuit>, With<PendingBehaviorPursuit>)>,
        ),
    >,
    transforms: TransformHelper,
) {
    for (actor, (task, running), has_agent, pending, poll, destination, direct) in
        behavior_action_actors(&mut actors, activation.0)
    {
        let running = task
            .zip(running)
            .filter(|(task, (instruction, _))| instruction.is_current(task));
        let owned = running
            .zip(pending.as_deref())
            .is_some_and(|((task, (instruction, _)), pending)| pending.owns(task, instruction));
        if let Some(stale) = pending.as_deref().filter(|_| !owned) {
            stop_pursuit_navigation(actor, stale.leg, destination, &mut commands);
            commands
                .entity(actor)
                .remove::<(PendingBehaviorPursuit, BehaviorPursuitPoll)>();
        }
        let Some((task, (instruction, running))) = running else {
            continue;
        };
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        let action = &running.0;
        if task.next_action_tick > clock.tick {
            continue;
        }
        let mut pending = pending.filter(|_| owned);
        let Some(target) = task.target().filter(|_| has_agent) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let mut poll = poll.filter(|_| owned);
        let mut initial_poll = BehaviorPursuitPoll::from_remaining_seconds(0.0);
        let current_poll = poll.as_deref_mut().unwrap_or(&mut initial_poll);
        let poll_due = current_poll.is_due(time.delta());
        let needs_leg = pending.as_ref().is_none_or(|pending| {
            matches!(
                pending.leg,
                BehaviorPursuitLeg::RouteArrived | BehaviorPursuitLeg::DirectArrived { .. }
            )
        });
        if !poll_due && !needs_leg {
            continue;
        }
        let (Ok(actor_pose), Ok(target_pose)) = (
            transforms.compute_global_transform(actor),
            transforms.compute_global_transform(target),
        ) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let target_position = target_pose.translation();
        let distance_squared = actor_pose
            .translation()
            .xz()
            .distance_squared(target_position.xz());
        let hit_radius_m = metres(action.hit_radius_cm);
        let pursuit_radius_m = metres(action.pursuit_radius_cm);
        let failure_radius_m = metres(action.failure_radius_cm);
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let mut request = PursuitRequests {
            actor,
            target,
            target_position,
            hit_radius_m,
            generation: &mut generation,
            route_requests: &mut route_requests,
            pursuit_requests: &mut pursuit_requests,
        };
        // The first route is held back while a direct request in this same
        // update may still make it unnecessary.
        let (mut state, mut unsent_first_route) = pending.as_deref().copied().map_or_else(
            || {
                let request_id = request.generation.advance();
                (
                    PendingBehaviorPursuit {
                        instruction: instruction.id(),
                        leg: BehaviorPursuitLeg::Route { request_id },
                    },
                    Some(request_id),
                )
            },
            |state| (state, None),
        );
        if poll_due {
            if failure_radius_m * failure_radius_m < distance_squared {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            *current_poll =
                BehaviorPursuitPoll::from_remaining_seconds(action.poll_interval_seconds);
            match state.leg {
                BehaviorPursuitLeg::Route { request_id }
                    if distance_squared < pursuit_radius_m * pursuit_radius_m =>
                {
                    // A refusal keeps the running route, or starts the first one.
                    unsent_first_route = None;
                    state.leg = BehaviorPursuitLeg::DirectRequested {
                        request_id: request.direct(None, request_id, pursuit_radius_m * 0.5),
                        route_request_id: request_id,
                    };
                }
                // Repeating the request re-tests the line it steers along.
                BehaviorPursuitLeg::Direct { request_id, .. }
                    if direct.is_some_and(|pursuit| pursuit.request_id == request_id) =>
                {
                    let refused_route_request_id = request.generation.advance();
                    request.direct(Some(request_id), refused_route_request_id, 0.0);
                    state.leg = BehaviorPursuitLeg::Direct {
                        request_id,
                        refused_route_request_id: Some(refused_route_request_id),
                    };
                }
                _ => {}
            }
        }
        match state.leg {
            // Arrival ended the steering short of the hit radius; it resumes.
            BehaviorPursuitLeg::DirectArrived { .. } => {
                let route_request_id = request.generation.advance();
                state.leg = BehaviorPursuitLeg::DirectRequested {
                    request_id: request.direct(None, route_request_id, 0.0),
                    route_request_id,
                };
            }
            BehaviorPursuitLeg::RouteArrived
                if distance_squared < pursuit_radius_m * pursuit_radius_m =>
            {
                let route_request_id = request.generation.advance();
                state.leg = BehaviorPursuitLeg::DirectRequested {
                    request_id: request.direct(None, route_request_id, pursuit_radius_m * 0.5),
                    route_request_id,
                };
            }
            BehaviorPursuitLeg::RouteArrived => {
                let request_id = request.generation.advance();
                request.route(request_id, pursuit_radius_m * 0.5);
                state.leg = BehaviorPursuitLeg::Route { request_id };
            }
            _ => {}
        }
        if let Some(request_id) = unsent_first_route {
            request.route(request_id, pursuit_radius_m * 0.5);
        }
        if let Some(mut pending) = pending.take() {
            pending.set_if_neq(state);
        } else {
            commands.entity(actor).insert(state);
        }
        if poll.is_none() {
            commands.entity(actor).insert(initial_poll);
        }
    }
}

/// Reads locomotion's answer to this step's direct requests, so a direct leg
/// that starts inside the hit radius completes in the same step.
#[allow(clippy::type_complexity)]
fn settle_pursuits_after_navigation_acceptance(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut pursuers: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorPursuit,
            ),
            &mut PendingBehaviorPursuit,
            &Destination,
        ),
        (Changed<Destination>, Without<PendingBehaviorTaskFailure>),
    >,
    transforms: TransformHelper,
) {
    for (actor, (task, instruction, running), mut pending, destination) in &mut pursuers {
        if !pending.owns(task, instruction) {
            continue;
        }
        let leg = pending.leg.settled_by_destination(destination.request_id);
        if leg == pending.leg {
            continue;
        }
        let newly_direct = matches!(pending.leg, BehaviorPursuitLeg::DirectRequested { .. })
            && matches!(leg, BehaviorPursuitLeg::Direct { .. });
        pending.leg = leg;
        if newly_direct && pursuit_hit(actor, task, running, &transforms) {
            finish_pursuit(
                actor,
                instruction,
                clock.tick,
                leg,
                Some(destination),
                &mut commands,
            );
        }
    }
}

#[allow(clippy::type_complexity)]
fn complete_pursuit_hits_after_motion(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    pursuers: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorPursuit,
            &PendingBehaviorPursuit,
            Option<&Destination>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    transforms: TransformHelper,
) {
    for (actor, task, instruction, running, pending, destination) in &pursuers {
        let BehaviorPursuitLeg::Direct { request_id, .. } = pending.leg else {
            continue;
        };
        if pending.owns(task, instruction)
            && destination.is_none_or(|destination| destination.request_id == request_id)
            && pursuit_hit(actor, task, running, &transforms)
        {
            finish_pursuit(
                actor,
                instruction,
                clock.tick,
                pending.leg,
                destination,
                &mut commands,
            );
        }
    }
}

fn pursuit_hit(
    actor: Entity,
    task: &BehaviorTaskExecutionState,
    running: &RunBehaviorPursuit,
    transforms: &TransformHelper,
) -> bool {
    let Some(target) = task.target() else {
        return false;
    };
    let (Ok(actor_pose), Ok(target_pose)) = (
        transforms.compute_global_transform(actor),
        transforms.compute_global_transform(target),
    ) else {
        return false;
    };
    actor_pose
        .translation()
        .xz()
        .distance_squared(target_pose.translation().xz())
        < metres(running.0.hit_radius_cm).powi(2)
}

fn finish_pursuit(
    actor: Entity,
    instruction: &RunningBehaviorInstruction,
    tick: u64,
    leg: BehaviorPursuitLeg,
    destination: Option<&Destination>,
    commands: &mut Commands,
) {
    stop_pursuit_navigation(actor, leg, destination, commands);
    commands
        .entity(actor)
        .remove::<(PendingBehaviorPursuit, BehaviorPursuitPoll)>();
    instruction.complete(actor, tick, commands);
}

/// A finished or replaced task leaves no pursuit steering its actor.
fn discard_pursuit_on_task_removal(
    removal: On<Remove, BehaviorTaskExecutionState>,
    mut commands: Commands,
    pursuers: Query<(&PendingBehaviorPursuit, Option<&Destination>)>,
) {
    if let Ok((pending, destination)) = pursuers.get(removal.entity) {
        stop_pursuit_navigation(removal.entity, pending.leg, destination, &mut commands);
        commands
            .entity(removal.entity)
            .try_remove::<(PendingBehaviorPursuit, BehaviorPursuitPoll)>();
    }
}

/// Locomotion drops the live target once its destination is gone.
fn stop_pursuit_navigation(
    actor: Entity,
    leg: BehaviorPursuitLeg,
    destination: Option<&Destination>,
    commands: &mut Commands,
) {
    if destination.is_some_and(|destination| {
        leg.navigation_request_ids()
            .contains(&Some(destination.request_id))
    }) {
        commands.entity(actor).try_remove::<Destination>();
    }
}

/// The navigation requests one pursuer's update may make.
struct PursuitRequests<'a, 'route, 'pursuit> {
    actor: Entity,
    target: Entity,
    target_position: Vec3,
    hit_radius_m: f32,
    generation: &'a mut NavigationRequestGeneration,
    route_requests: &'a mut MessageWriter<'route, NavigateTo>,
    pursuit_requests: &'a mut MessageWriter<'pursuit, PursueEntity>,
}

impl PursuitRequests<'_, '_, '_> {
    fn route(&mut self, request_id: u64, arrival_radius_m: f32) {
        self.route_requests.write(NavigateTo {
            entity: self.actor,
            request_id,
            destination: self.target_position,
            arrival_radius_m,
        });
    }

    /// Requests direct steering, or re-tests the active request's line, with
    /// the route that runs if the line is refused.
    fn direct(
        &mut self,
        active_request_id: Option<u64>,
        refused_route_request_id: u64,
        refused_route_arrival_radius_m: f32,
    ) -> u64 {
        let request_id = active_request_id.unwrap_or_else(|| self.generation.advance());
        self.pursuit_requests.write(PursueEntity {
            entity: self.actor,
            request_id,
            target: self.target,
            arrival_radius_m: self.hit_radius_m,
            refused_route: PursuitRefusalRoute {
                request_id: refused_route_request_id,
                arrival_radius_m: refused_route_arrival_radius_m,
            },
        });
        request_id
    }
}

#[allow(clippy::cast_precision_loss)]
const fn metres(centimetres: u32) -> f32 {
    centimetres as f32 * 0.01
}

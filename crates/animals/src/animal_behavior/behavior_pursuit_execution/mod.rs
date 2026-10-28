use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::behavior::action::movement::BehaviorPursuitAction;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_behavior::behavior_pursuit_types::BehaviorPursuitLeg;
use openzt2_zoo_state::animal_behavior::behavior_pursuit_types::PendingBehaviorPursuit;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence;
use openzt2_zoo_state::locomotion::locomotion_types::PursueEntity;
use openzt2_zoo_state::locomotion::locomotion_types::PursuitRefusalRoute;
use openzt2_zoo_state::locomotion::NavigationRequestAcceptance;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

pub(super) fn register_pursuit_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        (
            record_pursuit_navigation_results,
            fail_pursuits_of_unavailable_targets,
            advance_current_pursuit_actions,
            discard_pursuits_without_tasks,
        )
            .chain()
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
    application.add_systems(
        FixedUpdate,
        settle_pursuits_after_navigation_acceptance
            .after(NavigationRequestAcceptance)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn current_pursuit_action<'a>(
    documents: &'a Assets<BehaviorDocumentAsset>,
    task: &BehaviorTaskExecutionState,
) -> Option<&'a BehaviorPursuitAction> {
    match documents
        .get(&task.document)
        .and_then(|document| find_current_behavior_task_action(document, task))
    {
        Some(BehaviorAction::Pursuit(action)) => Some(action),
        _ => None,
    }
}

/// Applies locomotion's arrivals and failures to the legs the pursuit owns.
/// A refused direct line is answered by its route, so it is not a failure.
fn record_pursuit_navigation_results(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut pursuers: Query<
        (&BehaviorTaskExecutionState, &mut PendingBehaviorPursuit),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for failure in failures.read() {
        let Ok((task, pending)) = pursuers.get(failure.entity) else {
            continue;
        };
        if !pending.still_owns_exact_task_frame(task)
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
        let Ok((task, mut pending)) = pursuers.get_mut(arrival.entity) else {
            continue;
        };
        if !pending.still_owns_exact_task_frame(task) {
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
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    pursuers: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            Option<&BehaviorTaskEffectiveScore>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    targets: Query<(
        Has<WorldEntityIsCrated>,
        Has<RelocatingPlacedObject>,
        Option<&BehaviorTaskExecutionState>,
        Option<&BehaviorTaskEffectiveScore>,
    )>,
) {
    for (pursuer, task, score) in &pursuers {
        if task.next_action_tick > clock.tick || current_pursuit_action(&documents, task).is_none()
        {
            continue;
        }
        let Some(Ok((crated, relocating, target_task, target_score))) =
            task.target().map(|target| targets.get(target))
        else {
            continue;
        };
        let outscored = target_task.is_some_and(|target_task| {
            let target = BehaviorTaskEffectiveScore::of_running_task(target_score, target_task);
            target > 0.0 && target > BehaviorTaskEffectiveScore::of_running_task(score, task)
        });
        if crated || relocating || outscored {
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
    mut commands: Commands,
    clock: Res<ZooClock>,
    time: Res<Time<Fixed>>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut route_requests: MessageWriter<NavigateTo>,
    mut pursuit_requests: MessageWriter<PursueEntity>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &GlobalTransform,
            Has<NavAgent>,
            Option<&mut PendingBehaviorPursuit>,
            Option<&Destination>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    transforms: Query<&GlobalTransform>,
) {
    for (actor, mut task, transform, has_agent, pending, destination) in &mut actors {
        let action = current_pursuit_action(&documents, &task);
        let owned = action.is_some()
            && pending
                .as_deref()
                .is_some_and(|pending| pending.still_owns_exact_task_frame(&task));
        if let Some(stale) = pending.as_deref().filter(|_| !owned) {
            stop_pursuit_navigation(actor, stale.leg, destination, &mut commands);
            commands.entity(actor).remove::<PendingBehaviorPursuit>();
        }
        let Some(action) = action else {
            continue;
        };
        if task.next_action_tick > clock.tick {
            continue;
        }
        let mut pending = pending.filter(|_| owned);
        let (Some(target), Some(target_position)) = (
            task.target(),
            task.target()
                .and_then(|target| transforms.get(target).ok())
                .map(GlobalTransform::translation),
        ) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if !has_agent {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let distance_squared = (target_position - transform.translation())
            .xz()
            .length_squared();
        let hit_radius_m = metres(action.hit_radius_cm);
        let pursuit_radius_m = metres(action.pursuit_radius_cm);
        let failure_radius_m = metres(action.failure_radius_cm);
        let mut request = PursuitRequests {
            actor,
            target,
            target_position,
            hit_radius_m,
            sequence: &mut *sequence,
            route_requests: &mut route_requests,
            pursuit_requests: &mut pursuit_requests,
        };
        // The first route is held back while a direct request in this same
        // update may still make it unnecessary.
        let (mut state, mut unsent_first_route) = pending.as_deref().cloned().map_or_else(
            || {
                let request_id = request.sequence.next();
                (
                    PendingBehaviorPursuit {
                        execution_id: task.execution_id,
                        origin: task.create_return_frame_after_action(task.action),
                        stack_depth: task.stack.len(),
                        poll_remaining_seconds: 0.0,
                        leg: BehaviorPursuitLeg::Route { request_id },
                    },
                    Some(request_id),
                )
            },
            |state| (state, None),
        );
        if let Some(destination) = destination {
            state.leg = state.leg.settled_by_destination(destination.request_id);
        }
        state.poll_remaining_seconds -= time.delta_secs();
        if state.poll_remaining_seconds < 0.0 {
            if failure_radius_m * failure_radius_m < distance_squared {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            state.poll_remaining_seconds = action.poll_interval_seconds;
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
                BehaviorPursuitLeg::Direct {
                    request_id,
                    refused_route_request_id: None,
                } => {
                    let refused_route_request_id = request.sequence.next();
                    request.direct(Some(request_id), refused_route_request_id, 0.0);
                    state.leg = BehaviorPursuitLeg::Direct {
                        request_id,
                        refused_route_request_id: Some(refused_route_request_id),
                    };
                }
                _ => {}
            }
        }
        let within_hit_radius = distance_squared < hit_radius_m * hit_radius_m;
        match state.leg {
            BehaviorPursuitLeg::Direct { .. } | BehaviorPursuitLeg::DirectArrived { .. }
                if within_hit_radius =>
            {
                stop_pursuit_navigation(actor, state.leg, destination, &mut commands);
                commands.entity(actor).remove::<PendingBehaviorPursuit>();
                advance_behavior_task_to_next_action(&mut task, clock.tick);
                continue;
            }
            // Arrival ended the steering short of the hit radius; it resumes.
            BehaviorPursuitLeg::DirectArrived { .. } => {
                let route_request_id = request.sequence.next();
                state.leg = BehaviorPursuitLeg::DirectRequested {
                    request_id: request.direct(None, route_request_id, 0.0),
                    route_request_id,
                };
            }
            BehaviorPursuitLeg::RouteArrived
                if distance_squared < pursuit_radius_m * pursuit_radius_m =>
            {
                let route_request_id = request.sequence.next();
                state.leg = BehaviorPursuitLeg::DirectRequested {
                    request_id: request.direct(None, route_request_id, pursuit_radius_m * 0.5),
                    route_request_id,
                };
            }
            BehaviorPursuitLeg::RouteArrived => {
                let request_id = request.sequence.next();
                request.route(request_id, pursuit_radius_m * 0.5);
                state.leg = BehaviorPursuitLeg::Route { request_id };
            }
            _ => {}
        }
        if let Some(request_id) = unsent_first_route {
            request.route(request_id, pursuit_radius_m * 0.5);
        }
        if let Some(pending) = pending.as_deref_mut() {
            *pending = state;
        } else {
            commands.entity(actor).insert(state);
        }
    }
}

/// Reads locomotion's answer to this step's direct requests, so a direct leg
/// that starts inside the hit radius completes in the same step.
#[allow(clippy::type_complexity)]
fn settle_pursuits_after_navigation_acceptance(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut pursuers: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &GlobalTransform,
            &mut PendingBehaviorPursuit,
            &Destination,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    transforms: Query<&GlobalTransform>,
) {
    for (actor, mut task, transform, mut pending, destination) in &mut pursuers {
        let Some(action) = current_pursuit_action(&documents, &task) else {
            continue;
        };
        if !pending.still_owns_exact_task_frame(&task) {
            continue;
        }
        let leg = pending.leg.settled_by_destination(destination.request_id);
        if leg != pending.leg {
            pending.leg = leg;
        }
        let BehaviorPursuitLeg::Direct { request_id, .. } = leg else {
            continue;
        };
        let Some(target_position) = task
            .target()
            .and_then(|target| transforms.get(target).ok())
            .map(GlobalTransform::translation)
        else {
            continue;
        };
        let hit_radius_m = metres(action.hit_radius_cm);
        if (target_position - transform.translation())
            .xz()
            .length_squared()
            < hit_radius_m * hit_radius_m
            && destination.request_id == request_id
        {
            commands
                .entity(actor)
                .remove::<(PendingBehaviorPursuit, Destination)>();
            advance_behavior_task_to_next_action(&mut task, clock.tick);
        }
    }
}

/// A finished or replaced task leaves no pursuit steering its actor.
fn discard_pursuits_without_tasks(
    mut commands: Commands,
    pursuers: Query<
        (Entity, &PendingBehaviorPursuit, Option<&Destination>),
        Without<BehaviorTaskExecutionState>,
    >,
) {
    for (actor, pending, destination) in &pursuers {
        stop_pursuit_navigation(actor, pending.leg, destination, &mut commands);
        commands.entity(actor).remove::<PendingBehaviorPursuit>();
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
        commands.entity(actor).remove::<Destination>();
    }
}

/// The navigation requests one pursuer's update may make.
struct PursuitRequests<'a, 'w> {
    actor: Entity,
    target: Entity,
    target_position: Vec3,
    hit_radius_m: f32,
    sequence: &'a mut NavigationRequestSequence,
    route_requests: &'a mut MessageWriter<'w, NavigateTo>,
    pursuit_requests: &'a mut MessageWriter<'w, PursueEntity>,
}

impl PursuitRequests<'_, '_> {
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
        let request_id = active_request_id.unwrap_or_else(|| self.sequence.next());
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

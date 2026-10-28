use super::super::behavior_snapshot_types::{
    BehaviorActorSnapshot, BehaviorSnapshotEntityReference,
};
use super::parameters::{BehaviorSnapshotApplication, PreparedBehaviorActor};
use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_natural_water_target_types::{NaturalWaterTargetKind, SelectedNaturalWaterTarget},
        behavior_pursuit_types::PendingBehaviorPursuit,
        behavior_random_stream_state::BehaviorRandomStream,
        behavior_synchronized_set_types::SynchronizedBehaviorOwner,
        behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
    },
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorDockingCompletion,
        PendingBehaviorMoveCompletion, PendingBehaviorTaskFailure, SynchronizedBehaviorParticipant,
    },
    locomotion::locomotion_types::{
        Destination, Docking, EntityPursuit, MotionProgress, Route, SpatialCell, Steering,
        SwimLayer,
    },
    staff::{
        staff_employment_types::AvailableForWork,
        staff_job_types::{CurrentJob, JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask},
    },
};
use std::collections::BTreeMap;

pub(super) fn apply_prepared_behavior_actor(
    commands: &mut Commands,
    record: &BehaviorActorSnapshot,
    prepared: PreparedBehaviorActor,
    identities: &BTreeMap<BehaviorSnapshotEntityReference, Entity>,
    application: &mut BehaviorSnapshotApplication,
) {
    let resolve = |reference: &BehaviorSnapshotEntityReference| identities[reference];
    let actor = prepared.actor;
    let execution_id = record.task.execution_id;
    let current = prepared.current;
    let task = BehaviorTaskExecutionState {
        execution_id,
        program: current.program,
        document: current.document,
        declaration: current.declaration,
        phase: current.phase,
        action: current.action,
        repetitions: current.repetitions,
        target: record.task.target.as_ref().map(resolve),
        object: record.task.object.as_ref().map(resolve),
        next_action_tick: record.task.next_action_tick,
        interaction_slot: record.task.interaction_slot,
        stack: prepared.stack,
    };
    application.executions.reserve_restored(execution_id);
    let mut entity = commands.entity(actor);
    entity.remove::<(
        BehaviorTaskEffectiveScore,
        PendingBehaviorTaskFailure,
        PendingBehaviorMoveCompletion,
        PendingBehaviorDockingCompletion,
        PendingBehaviorPursuit,
        Destination,
        Docking,
        EntityPursuit,
        SelectedNaturalWaterTarget,
    )>();
    if let Some(score) = &record.task.score {
        entity.insert(BehaviorTaskEffectiveScore {
            execution_id,
            template_score: score.template.map(f32::from_bits),
            linked_score: score.linked.map(f32::from_bits),
            selected_candidate_score: score.selected_candidate.map(f32::from_bits),
        });
    }
    if record.task.failing {
        entity.insert(PendingBehaviorTaskFailure);
    }
    if let Some(random) = record.random {
        entity.insert(BehaviorRandomStream(DeterministicRng::from_raw(random)));
    }
    if let Some(water) = &record.water_target {
        entity.insert(SelectedNaturalWaterTarget {
            region: resolve(&water.region),
            water: Vec3::from_array(water.water),
            approach: Vec3::from_array(water.approach),
            kind: water.depth.map_or(NaturalWaterTargetKind::Shore, |depth| {
                NaturalWaterTargetKind::Surface { depth }
            }),
        });
    }
    if let Some(nav) = &record.navigation {
        let mut route = Route::with_capacity(
            application
                .capacity
                .as_ref()
                .expect("validated navigation capacity")
                .max_route_points,
        );
        route.points.extend_from_slice(&nav.route.points);
        route.cursor = nav.route.cursor;
        entity.insert((
            nav.agent,
            nav.mode,
            route,
            Steering::default(),
            SpatialCell::OUTSIDE,
            nav.contact_steering.unwrap_or_default(),
            nav.progress.unwrap_or(MotionProgress {
                last_position: application
                    .entities
                    .get(actor)
                    .ok()
                    .and_then(|actor| {
                        actor
                            .get::<GlobalTransform>()
                            .map(GlobalTransform::translation)
                    })
                    .unwrap_or(Vec3::ZERO),
                stationary_ticks: 0,
            }),
        ));
        if application
            .entities
            .get(actor)
            .is_ok_and(|actor| actor.contains::<avian3d::prelude::RigidBody>())
        {
            entity
                .insert(avian3d::prelude::LinearVelocity(nav.velocity.0))
                .remove::<openzt2_zoo_state::locomotion::locomotion_types::Velocity>();
        } else {
            entity.insert(nav.velocity);
        }
        if let Some(layer) = nav.swim_layer {
            entity.insert(layer);
        } else {
            entity.remove::<SwimLayer>();
        }
        if let Some(contact) = nav.water_contact {
            entity.insert(contact);
        }
        if let Some(destination) = nav.destination {
            application
                .requests
                .reserve_restored(destination.request_id);
            entity.insert(destination);
            if nav.pursuit_target.is_none() {
                entity.insert(
                    openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute {
                        request_id: destination.request_id,
                    },
                );
            }
            if let Some(target) = &nav.pursuit_target {
                entity.insert(EntityPursuit {
                    request_id: destination.request_id,
                    target: resolve(target),
                });
            }
        }
        if let Some(dock) = &nav.docking {
            application.requests.reserve_restored(dock.request_id);
            entity.insert(Docking {
                request_id: dock.request_id,
                target: resolve(&dock.target),
                point: Vec3::from_array(dock.point),
                forward: Vec3::from_array(dock.forward),
                radius_m: dock.radius_m,
            });
        }
        if let (Some(pending), Some(origin)) = (&nav.move_completion, prepared.move_origin) {
            application.requests.reserve_restored(pending.request_id);
            if nav.destination.is_some()
                || nav
                    .pending_results
                    .iter()
                    .any(|result| result.request_id() == pending.request_id)
            {
                entity.insert(PendingBehaviorMoveCompletion {
                    origin,
                    stack_depth: pending.stack_depth,
                    request_id: pending.request_id,
                });
            }
        }
        if let (Some(pending), Some(origin)) = (&nav.docking_completion, prepared.dock_origin) {
            application.requests.reserve_restored(pending.request_id);
            if nav.destination.is_some()
                || nav.docking.is_some()
                || nav
                    .pending_results
                    .iter()
                    .any(|result| result.request_id() == pending.request_id)
            {
                entity.insert(PendingBehaviorDockingCompletion {
                    origin,
                    stack_depth: pending.stack_depth,
                    request_id: pending.request_id,
                    target: resolve(pending.target.as_ref().expect("validated docking target")),
                });
            }
        }
        if let (Some(pending), Some(origin)) = (&nav.pursuit, prepared.pursuit_origin) {
            for request in pending.leg.navigation_request_ids().into_iter().flatten() {
                application.requests.reserve_restored(request);
            }
            let leg = reissue_unaccepted_pursuit_navigation(actor, nav, &task, application);
            entity.insert(PendingBehaviorPursuit {
                execution_id,
                origin,
                stack_depth: pending.stack_depth,
                poll_remaining_seconds: pending.poll_remaining_seconds,
                leg,
            });
        }
        for result in &nav.pending_results {
            use super::super::behavior_snapshot_types::BehaviorNavigationResultSnapshot;
            match result {
                BehaviorNavigationResultSnapshot::Arrived { request_id, target } => {
                    application.arrivals.write(
                        openzt2_zoo_state::locomotion::locomotion_types::Arrived {
                            entity: actor,
                            request_id: *request_id,
                            target: target.as_ref().map(resolve),
                        },
                    );
                }
                BehaviorNavigationResultSnapshot::Failed { request_id, reason } => {
                    application.failures.write(
                        openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed {
                            entity: actor,
                            request_id: *request_id,
                            reason: *reason,
                        },
                    );
                }
            }
        }
    }
    entity.insert(task);
    if let (Some(owner), Some(origin)) = (&record.synchronized, prepared.synchronized_origin) {
        let target = resolve(&owner.target);
        entity.insert(SynchronizedBehaviorOwner {
            execution_id,
            target_execution_id: owner.target_execution_id,
            target,
            origin,
            depth: owner.depth,
            target_outcome: owner.target_outcome,
        });
        commands
            .entity(target)
            .insert(SynchronizedBehaviorParticipant {
                owner: actor,
                execution_id: owner.target_execution_id,
            });
    }
    if let (Some(job), Some(frame)) = (&record.staff_job, prepared.job_frame) {
        let job_entity = commands
            .spawn((
                StaffJob {
                    kind: job.kind,
                    target: resolve(&job.target),
                    urgency: job.urgency,
                    token: job.token,
                },
                JobClaim { staff: actor },
                StaffJobBehaviorTask {
                    document: frame.document,
                    task: frame.program,
                    declaration_index: frame.declaration,
                    execution_id: Some(execution_id),
                },
            ))
            .id();
        if job.progressing {
            commands.entity(job_entity).insert(JobProgress);
        }
        commands
            .entity(actor)
            .insert(CurrentJob { job: job_entity })
            .remove::<AvailableForWork>();
    }
}

fn reissue_unaccepted_pursuit_navigation(
    actor: Entity,
    nav: &super::super::behavior_snapshot_types::BehaviorOwnedNavigationSnapshot,
    task: &BehaviorTaskExecutionState,
    application: &mut BehaviorSnapshotApplication,
) -> openzt2_zoo_state::animal_behavior::behavior_pursuit_types::BehaviorPursuitLeg {
    use openzt2_zoo_state::animal_behavior::behavior_pursuit_types::BehaviorPursuitLeg;
    use openzt2_zoo_state::locomotion::locomotion_types::{
        NavigateTo, PursueEntity, PursuitRefusalRoute,
    };
    let pending = nav.pursuit.as_ref().expect("validated pursuit record");
    if nav.destination.is_some() || nav.pending_results.iter().any(|result| {
        use super::super::behavior_snapshot_types::BehaviorNavigationResultSnapshot;
        pending.leg.navigation_request_ids().contains(&Some(result.request_id()))
            && !matches!(result, BehaviorNavigationResultSnapshot::Failed { request_id, reason }
                if *reason == openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure::NoRoute
                    && pending.leg.is_direct_request(*request_id))
    }) { return pending.leg; }
    let target = task.target.expect("validated pursuit target");
    let document = application
        .documents
        .get(&task.document)
        .expect("validated pursuit document");
    let Some(openzt2_game_data::behavior::action_record::BehaviorAction::Pursuit(action)) =
        openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action(
            document, task,
        )
    else {
        unreachable!("validated pursuit action");
    };
    let position = application.entities.get(target).ok().and_then(|target| {
        target
            .get::<GlobalTransform>()
            .map(GlobalTransform::translation)
    });
    match pending.leg {
        BehaviorPursuitLeg::Route { request_id } => {
            if let Some(destination) = position {
                application.route_requests.write(NavigateTo {
                    entity: actor,
                    request_id,
                    destination,
                    arrival_radius_m: action.pursuit_radius_cm as f32 * 0.005,
                });
            }
        }
        BehaviorPursuitLeg::DirectRequested {
            request_id,
            route_request_id,
        }
        | BehaviorPursuitLeg::Direct {
            request_id,
            refused_route_request_id: Some(route_request_id),
        } => {
            application.pursuit_requests.write(PursueEntity {
                entity: actor,
                request_id,
                target,
                arrival_radius_m: action.hit_radius_cm as f32 * 0.01,
                refused_route: PursuitRefusalRoute {
                    request_id: route_request_id,
                    arrival_radius_m: action.pursuit_radius_cm as f32 * 0.005,
                },
            });
        }
        BehaviorPursuitLeg::Direct {
            request_id,
            refused_route_request_id: None,
        } => {
            let route_request_id = application.requests.next();
            application.pursuit_requests.write(PursueEntity {
                entity: actor,
                request_id,
                target,
                arrival_radius_m: action.hit_radius_cm as f32 * 0.01,
                refused_route: PursuitRefusalRoute {
                    request_id: route_request_id,
                    arrival_radius_m: 0.0,
                },
            });
            return BehaviorPursuitLeg::Direct {
                request_id,
                refused_route_request_id: Some(route_request_id),
            };
        }
        BehaviorPursuitLeg::RouteArrived | BehaviorPursuitLeg::DirectArrived { .. } => {}
    }
    pending.leg
}

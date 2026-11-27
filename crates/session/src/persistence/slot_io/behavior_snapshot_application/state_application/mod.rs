use super::super::behavior_snapshot_identity_high_water_marks::maximum_owned_navigation_request;
use super::super::behavior_snapshot_types::{
    BehaviorSnapshotEntityReference, BehaviorTaskActorSnapshot,
};
use super::super::restored_component_values::restore_optional_component;
use super::parameters::{BehaviorSnapshotApplication, PreparedBehaviorActor};
use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_natural_water_target_types::{NaturalWaterTargetKind, SelectedNaturalWaterTarget},
        behavior_pursuit_types::{BehaviorPursuitLeg, BehaviorPursuitPoll, PendingBehaviorPursuit},
        behavior_random_stream_state::BehaviorRandomStream,
        behavior_synchronized_set_types::{
            SynchronizedBehaviorOperation, SynchronizedBehaviorPartner, SynchronizedBehaviorSubject,
        },
        behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
    },
    behavior_task_execution_types::{
        running_actions::{
            FollowingEntity, RestoredSplashAnimation, RunningBehaviorEconomyTransaction,
            RunningBehaviorInstruction, SplineDockMotion, SplineDockPhase, SplineDockRetry,
        },
        BehaviorTaskExecutionState, PendingBehaviorDockingCompletion,
        PendingBehaviorMoveCompletion, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Destination, Docking, EntityPursuit, MotionProgress, NavigationRequestGeneration, Route,
        Steering,
    },
    staff::staff_job_types::{
        JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask, StaffJobRouting, StaffJobTarget,
        UnsuccessfulStaffJobCandidates,
    },
};
use std::collections::BTreeMap;

#[derive(Component, Default)]
pub(in crate::persistence) struct RestoredBehaviorInstructionState {
    pursuit: Option<(BehaviorPursuitLeg, f32)>,
    economy: Option<RestoredBehaviorEconomyTransaction>,
    movement: Option<u64>,
    docking: Option<(u64, Entity)>,
    following: Option<(Entity, u64, u64)>,
    spline: Option<(Entity, SplineDockPhase, bool, Option<f32>)>,
}

struct RestoredBehaviorEconomyTransaction {
    facility: Entity,
    target: Option<Entity>,
    transaction: openzt2_game_data::AssetId,
    cost_override: Option<f32>,
    next_transaction: Option<openzt2_game_data::AssetId>,
}

pub(super) fn apply_prepared_behavior_actor(
    commands: &mut Commands,
    record: &BehaviorTaskActorSnapshot,
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
        depth: prepared.stack.len(),
    };
    if let Ok(mut generation) = application.executions.get_mut(actor) {
        generation.reserve_restored(execution_id);
    } else {
        commands.entity(actor).insert(
            openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskGeneration::from_restored(
                execution_id,
            ),
        );
    }
    commands
        .entity(actor)
        .remove::<BehaviorTaskExecutionState>();
    for (frame, saved) in prepared.stack.into_iter().zip(&record.task.stack) {
        openzt2_zoo_state::behavior_task_execution_types::continuations::suspend_behavior_frame(
            actor,
            frame,
            saved
                .resumes_with_target
                .as_ref()
                .map(|target| target.as_ref().map(resolve)),
            commands,
        );
    }
    let mut entity = commands.entity(actor);
    entity.remove::<(
        BehaviorTaskEffectiveScore,
        PendingBehaviorTaskFailure,
        PendingBehaviorMoveCompletion,
        PendingBehaviorDockingCompletion,
        PendingBehaviorPursuit,
        BehaviorPursuitPoll,
        Destination,
        Docking,
        EntityPursuit,
        SelectedNaturalWaterTarget,
        openzt2_zoo_state::behavior_task_execution_types::running_actions::RunningBehaviorEconomyTransaction,
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
    let mut instruction_state = RestoredBehaviorInstructionState::default();
    instruction_state.economy =
        record
            .transaction
            .as_ref()
            .map(|pending| RestoredBehaviorEconomyTransaction {
                facility: resolve(&pending.facility),
                target: task.target,
                transaction: pending.transaction,
                cost_override: pending.cost_override.map(f32::from_bits),
                next_transaction: pending.next_transaction,
            });
    if let Some(nav) = &record.navigation {
        let mut generation = application.requests.get(actor).copied().unwrap_or_default();
        generation.reserve_restored(maximum_owned_navigation_request(nav));
        if let Some(pending) = &nav.pursuit {
            let leg = reissue_unaccepted_pursuit_navigation(
                actor,
                nav,
                &task,
                &mut generation,
                application,
            );
            instruction_state.pursuit = Some((leg, pending.poll_remaining_seconds));
        }
        if let Ok(mut current) = application.requests.get_mut(actor) {
            if current.last_issued() < generation.last_issued() {
                *current = generation;
            }
        } else {
            entity.insert(generation);
        }
        let mut route = Route::with_capacity(nav.route.points.len());
        route.points.extend_from_slice(&nav.route.points);
        route.cursor = nav.route.cursor;
        let mut actors = application.actors.p1();
        let live = actors.get_mut(actor).expect("validated behavior actor");
        let progress = nav.progress.unwrap_or(MotionProgress {
            last_position: live
                .transform
                .map_or(Vec3::ZERO, GlobalTransform::translation),
            stationary_ticks: 0,
        });
        restore_optional_component(&mut entity, live.agent, Some(nav.agent), false);
        restore_optional_component(&mut entity, live.mode, Some(nav.mode), false);
        restore_optional_component(&mut entity, live.route, Some(route), false);
        restore_optional_component(&mut entity, live.steering, Some(Steering::default()), false);
        restore_optional_component(
            &mut entity,
            live.contact_steering,
            Some(nav.contact_steering.unwrap_or_default()),
            false,
        );
        restore_optional_component(&mut entity, live.progress, Some(progress), false);
        let rigid_body = live.has_rigid_body;
        restore_optional_component(
            &mut entity,
            live.velocity,
            (!rigid_body).then_some(nav.velocity),
            rigid_body,
        );
        restore_optional_component(
            &mut entity,
            live.linear_velocity,
            rigid_body.then_some(avian3d::prelude::LinearVelocity(nav.velocity.0)),
            false,
        );
        restore_optional_component(&mut entity, live.swim_layer, nav.swim_layer, true);
        restore_optional_component(&mut entity, live.water_contact, nav.water_contact, false);
        if let Some(destination) = nav.destination {
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
            entity.insert(Docking {
                request_id: dock.request_id,
                target: resolve(&dock.target),
                point: Vec3::from_array(dock.point),
                forward: Vec3::from_array(dock.forward),
                radius_m: dock.radius_m,
            });
        }
        let request_is_owned = |request_id| {
            nav.pending_results
                .iter()
                .any(|result| result.request_id() == request_id)
        };
        instruction_state.movement = nav
            .move_completion
            .as_ref()
            .filter(|pending| nav.destination.is_some() || request_is_owned(pending.request_id))
            .map(|pending| pending.request_id);
        instruction_state.docking = nav
            .docking_completion
            .as_ref()
            .filter(|pending| {
                nav.destination.is_some()
                    || nav.docking.is_some()
                    || request_is_owned(pending.request_id)
            })
            .map(|pending| {
                (
                    pending.request_id,
                    resolve(pending.target.as_ref().expect("validated docking target")),
                )
            });
        instruction_state.following = nav.follow.as_ref().map(|pending| {
            (
                resolve(&pending.target),
                pending.until_tick,
                pending.request_id,
            )
        });
        instruction_state.spline = nav.spline.as_ref().map(|pending| {
            let phase = if nav.destination.is_none()
                && pending
                    .phase
                    .request()
                    .is_some_and(|id| !request_is_owned(id))
            {
                SplineDockPhase::Ready
            } else {
                pending.phase
            };
            (
                resolve(&pending.target),
                phase,
                pending.owns_disabled_collider,
                pending.retry_seconds,
            )
        });
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
    if let Some(splash) = record.splash {
        entity.insert(RestoredSplashAnimation {
            seek_seconds: splash.seek_seconds,
            emitted: splash.emitted,
        });
    }
    if instruction_state.pursuit.is_some()
        || instruction_state.economy.is_some()
        || instruction_state.movement.is_some()
        || instruction_state.docking.is_some()
        || instruction_state.following.is_some()
        || instruction_state.spline.is_some()
    {
        entity.insert(instruction_state);
    }
    entity.insert(task);
    if let (Some(owner), Some(origin)) = (&record.synchronized, prepared.synchronized_origin) {
        let target = resolve(&owner.target);
        commands.spawn((
            SynchronizedBehaviorSubject(actor),
            SynchronizedBehaviorPartner(target),
            SynchronizedBehaviorOperation {
                execution_id,
                target_execution_id: owner.target_execution_id,
                origin,
                depth: owner.depth,
                target_outcome: owner.target_outcome,
            },
        ));
    }
    if let (Some(job), Some(frame)) = (&record.staff_job, prepared.job_frame) {
        let target = job.target.as_ref().map(resolve);
        let member = application
            .actors
            .p0()
            .get(actor)
            .ok()
            .and_then(|actor| actor.world_member)
            .copied();
        let mut job_entity = commands.spawn((
            StaffJob {
                kind: job.kind,
                target,
                point: job.point.map(Vec3::from_array),
                priority: job
                    .priority
                    .or_else(|| job.urgency.map(|urgency| f32::from_bits(u32::MAX - urgency)))
                    .filter(|priority| priority.is_finite() && *priority > 0.0)
                    .unwrap_or(openzt2_game_data::world_definitions::staff_management::StaffRequestData::CONSTRUCTED_DEFAULT.priority),
                issued_at_seconds: job.issued_at_seconds,
                remaining_wait_seconds: job.remaining_wait_seconds,
                first_attempt: job.first_attempt,
                token: job.token,
            },
            StaffJobRouting {
                subject_type: job.subject_type,
                recipient: job.recipient,
            },
            JobClaim { staff: actor },
            StaffJobBehaviorTask {
                document: frame.document,
                task: frame.program,
                declaration_index: frame.declaration,
                execution_id: Some(execution_id),
            },
        ));
        if let Some(target) = target {
            job_entity.insert(StaffJobTarget(target));
        }
        if let Some(member) = member {
            job_entity.insert(member);
        }
        if let Some(id) = job.persistent_identifier {
            job_entity
                .insert(openzt2_world_state::world_spawn::persistent_id_types::PersistentId(id));
        }
        if !job.rejected_staff.is_empty() {
            let mut rejected = UnsuccessfulStaffJobCandidates::default();
            for staff in &job.rejected_staff {
                rejected.record(resolve(staff));
            }
            job_entity.insert(rejected);
        }
        if job.progressing {
            job_entity.insert(JobProgress);
        }
    }
}

pub(in crate::persistence) fn bind_restored_behavior_instruction_state(
    mut commands: Commands,
    mut actors: Query<(
        Entity,
        &RestoredBehaviorInstructionState,
        Option<&RunningBehaviorInstruction>,
        Option<&mut RunningBehaviorEconomyTransaction>,
        Option<&mut FollowingEntity>,
        Option<&mut SplineDockMotion>,
        Option<&mut SplineDockRetry>,
    )>,
) {
    for (entity, saved, instruction, economy, following, spline, retry) in &mut actors {
        let Some(instruction) = instruction.map(RunningBehaviorInstruction::id) else {
            continue;
        };
        let mut actor = commands.entity(entity);
        actor.remove::<RestoredBehaviorInstructionState>();
        if let Some((leg, seconds)) = saved.pursuit {
            actor.insert((
                PendingBehaviorPursuit { instruction, leg },
                BehaviorPursuitPoll::from_remaining_seconds(seconds),
            ));
        }
        if let Some(saved) = &saved.economy {
            restore_optional_component(
                &mut actor,
                economy,
                Some(RunningBehaviorEconomyTransaction {
                    instruction,
                    facility: saved.facility,
                    target: saved.target,
                    transaction: saved.transaction,
                    cost_override: saved.cost_override,
                    payment: None,
                    next_transaction: saved.next_transaction,
                }),
                false,
            );
        }
        if let Some(request_id) = saved.movement {
            actor.insert(PendingBehaviorMoveCompletion {
                instruction,
                request_id,
            });
        }
        if let Some((target, until_tick, request_id)) = saved.following {
            restore_optional_component(
                &mut actor,
                following,
                Some(FollowingEntity {
                    instruction,
                    target,
                    until_tick,
                    request_id,
                }),
                false,
            );
        }
        if let Some((target, phase, owns_disabled_collider, retry_seconds)) = saved.spline {
            restore_optional_component(
                &mut actor,
                spline,
                Some(SplineDockMotion {
                    instruction,
                    target,
                    phase,
                    owns_disabled_collider,
                }),
                false,
            );
            restore_optional_component(
                &mut actor,
                retry,
                retry_seconds
                    .map(|seconds| SplineDockRetry(Timer::from_seconds(seconds, TimerMode::Once))),
                true,
            );
        }
        if let Some((request_id, target)) = saved.docking {
            actor.insert(PendingBehaviorDockingCompletion {
                instruction,
                target,
                request_id,
            });
        }
    }
}

fn reissue_unaccepted_pursuit_navigation(
    actor: Entity,
    nav: &super::super::behavior_snapshot_types::BehaviorOwnedNavigationSnapshot,
    task: &BehaviorTaskExecutionState,
    generation: &mut NavigationRequestGeneration,
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
    let Some(openzt2_game_data::behavior::action_record::BehaviorAction::Pursuit(action)) = task
        .phase
        .authored_action(document, task.declaration, task.action)
    else {
        unreachable!("validated pursuit action");
    };
    let position = application
        .actors
        .p0()
        .get(target)
        .ok()
        .and_then(|target| target.global_transform.map(GlobalTransform::translation));
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
            let route_request_id = generation.advance();
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

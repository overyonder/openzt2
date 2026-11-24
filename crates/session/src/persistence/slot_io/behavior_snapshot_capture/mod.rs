use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_zoo_state::{
    animal_behavior::behavior_natural_water_target_types::NaturalWaterTargetKind,
    animal_behavior::behavior_synchronized_set_types::{
        SynchronizedBehaviorOperation, SynchronizedBehaviorPartner,
    },
    animal_behavior::interaction_container_membership::InteractionContainerMemberships,
    behavior_task_execution_types::{BehaviorTaskExecutionState, BehaviorTaskReturnFrame},
    locomotion::locomotion_types::{Route, Velocity},
};

#[derive(SystemParam)]
pub(in crate::persistence) struct BehaviorSnapshotCapture<'w, 's> {
    rejections:
        openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections<
            'w,
            's,
        >,
    continuations:
        openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames<
            'w,
            's,
        >,
    entities: BehaviorSnapshotEntities<'w, 's>,
    arrivals: Res<'w, Messages<openzt2_zoo_state::locomotion::locomotion_types::Arrived>>,
    failures: Res<'w, Messages<openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed>>,
    memberships: InteractionContainerMemberships<'w, 's>,
    synchronizations: Query<
        'w,
        's,
        (
            &'static SynchronizedBehaviorOperation,
            &'static SynchronizedBehaviorPartner,
        ),
    >,
    documents:
        Res<'w, Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>>,
    finished:
        Res<'w, Messages<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished>>,
    failed: Res<'w, Messages<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed>>,
}

impl BehaviorSnapshotCapture<'_, '_> {
    pub(super) fn capture(
        &self,
    ) -> Result<Vec<BehaviorActorSnapshot>, WorldSnapshotPersistenceFailure> {
        let mut records = capture_behavior_actor_snapshots(
            &self.entities,
            &self.documents,
            &self.memberships,
            &self.synchronizations,
            &self.continuations,
        )?;
        let mut arrivals = self.arrivals.get_cursor();
        let mut failures = self.failures.get_cursor();
        let arrivals = arrivals.read(&self.arrivals).copied().collect::<Vec<_>>();
        let failures = failures.read(&self.failures).copied().collect::<Vec<_>>();
        let mut finished = self.finished.get_cursor();
        let mut failed = self.failed.get_cursor();
        let finished = finished.read(&self.finished).copied().collect::<Vec<_>>();
        let failed = failed.read(&self.failed).copied().collect::<Vec<_>>();
        let identities = self
            .entities
            .iter()
            .filter_map(|entity| entity.persistent_id.map(|id| (id.0, entity.entity)))
            .collect::<std::collections::BTreeMap<_, _>>();
        for record in &mut records {
            let BehaviorActorSnapshot::Task(record) = record else {
                continue;
            };
            let actor = *identities
                .get(&record.persistent_identifier)
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            if let Some(nav) = record.navigation.as_mut() {
                let owns = |request| {
                    nav.move_completion
                        .as_ref()
                        .is_some_and(|pending| pending.request_id == request)
                        || nav
                            .docking_completion
                            .as_ref()
                            .is_some_and(|pending| pending.request_id == request)
                        || nav.pursuit.as_ref().is_some_and(|pending| {
                            pending
                                .leg
                                .navigation_request_ids()
                                .contains(&Some(request))
                        })
                        || nav
                            .action_navigation_requests()
                            .any(|candidate| candidate == request)
                };
                let mut pending_results = Vec::new();
                for arrival in arrivals
                    .iter()
                    .filter(|arrival| arrival.entity == actor && owns(arrival.request_id))
                {
                    pending_results.push(
                        super::behavior_snapshot_types::BehaviorNavigationResultSnapshot::Arrived {
                            request_id: arrival.request_id,
                            target: arrival
                                .target
                                .map(|target| {
                                    capture_behavior_entity_reference(target, &self.entities)
                                })
                                .transpose()?,
                        },
                    );
                }
                for failure in failures
                    .iter()
                    .filter(|failure| failure.entity == actor && owns(failure.request_id))
                {
                    pending_results.push(
                        super::behavior_snapshot_types::BehaviorNavigationResultSnapshot::Failed {
                            request_id: failure.request_id,
                            reason: failure.reason,
                        },
                    );
                }
                nav.pending_results = pending_results;
            }
            if let Some(synchronized) = record.synchronized.as_mut() {
                let partner = self
                    .entities
                    .get(actor)
                    .ok()
                    .and_then(|actor| actor.synchronized_behavior_owner)
                    .and_then(|owner| owner.iter().next())
                    .and_then(|operation| self.synchronizations.get(operation).ok())
                    .map(|(_, partner)| partner.0)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                if finished.iter().any(|event| {
                    event.actor == partner && event.execution_id == synchronized.target_execution_id
                }) {
                    synchronized.target_outcome = Some(true);
                }
                if failed.iter().any(|event| {
                    event.actor == partner && event.execution_id == synchronized.target_execution_id
                }) {
                    synchronized.target_outcome = Some(false);
                }
            }
        }
        for actor in self.entities.iter().filter(|actor| !actor.guest) {
            let Some(id) = actor.persistent_id else {
                continue;
            };
            let mut retry_delays = self
                .rejections
                .snapshot(actor.entity)
                .filter_map(|(target, failures, remaining_updates)| {
                    // Only persist timers for targets restored with this zoo.
                    let target = self.entities.get(target).ok()?;
                    if target.guest {
                        return None;
                    }
                    Some(BehaviorTargetRetrySnapshot {
                        target: capture_behavior_entity_reference(target.entity, &self.entities)
                            .ok()?,
                        failures,
                        remaining_updates,
                    })
                })
                .collect::<Vec<_>>();
            if !retry_delays.is_empty() {
                retry_delays.sort_unstable_by(|a, b| a.target.cmp(&b.target));
                records.push(BehaviorActorSnapshot::Retry(BehaviorActorRetrySnapshot {
                    persistent_identifier: id.0,
                    retry_delays,
                }));
            }
        }
        records.sort_unstable_by_key(BehaviorActorSnapshot::sort_key);
        super::behavior_snapshot_section_encoding::validate_behavior_actor_snapshots(&records)?;
        Ok(records)
    }
}
use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    behavior_snapshot_entity_references::{
        capture_behavior_entity_reference, BehaviorSnapshotEntities, BehaviorSnapshotEntity,
    },
    behavior_snapshot_types::{
        BehaviorActorRetrySnapshot, BehaviorActorSnapshot, BehaviorDockingSnapshot,
        BehaviorFrameSnapshot, BehaviorNaturalWaterTargetSnapshot,
        BehaviorNavigationCompletionSnapshot, BehaviorOwnedNavigationSnapshot,
        BehaviorPursuitSnapshot, BehaviorStaffJobSnapshot, BehaviorSynchronizedSnapshot,
        BehaviorTargetRetrySnapshot, BehaviorTaskActorSnapshot, BehaviorTaskScoreSnapshot,
        BehaviorTaskSnapshot,
    },
};

fn capture_behavior_frame(
    frame: &BehaviorTaskReturnFrame,
    documents: &Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>,
) -> Result<BehaviorFrameSnapshot, WorldSnapshotPersistenceFailure> {
    let document = documents
        .get(&frame.document)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let subjects = if frame.phase
        == openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase::Set
    {
        &document
            .behavior_set_at_index(frame.declaration)
            .filter(|set| set.answers_program(frame.program))
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?
            .subjects
    } else {
        &document
            .behavior_task_at_index(frame.declaration)
            .filter(|task| task.id == frame.program)
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?
            .subjects
    };
    let mut subjects = subjects
        .iter()
        .map(|subject| openzt2_game_data::AssetId::from_key(subject))
        .collect::<Vec<_>>();
    subjects.sort_unstable();
    subjects.dedup();
    Ok(BehaviorFrameSnapshot {
        document: frame
            .document
            .path()
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?
            .to_string(),
        program: frame.program,
        phase: frame.phase,
        subjects,
        action: frame.action,
        repetitions: frame.repetitions,
    })
}

fn capture_behavior_actor_snapshots(
    entities: &BehaviorSnapshotEntities,
    documents: &Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>,
    memberships: &InteractionContainerMemberships,
    synchronizations: &Query<(&SynchronizedBehaviorOperation, &SynchronizedBehaviorPartner)>,
    continuations: &openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames,
) -> Result<Vec<BehaviorActorSnapshot>, WorldSnapshotPersistenceFailure> {
    let mut records = Vec::new();
    let reference = |entity| capture_behavior_entity_reference(entity, entities);
    for actor in entities {
        // Guests are not saved (#118), so their tasks would reference actors
        // that a load never restores.
        if actor.guest {
            continue;
        }
        let Some(task) = actor.behavior_task_execution_state else {
            continue;
        };
        let id = actor
            .persistent_id
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let score = actor
            .behavior_task_effective_score
            .filter(|score| score.execution_id == task.execution_id)
            .map(|score| BehaviorTaskScoreSnapshot {
                execution_id: score.execution_id,
                template: score.template_score.map(f32::to_bits),
                linked: score.linked_score.map(f32::to_bits),
                selected_candidate: score.selected_candidate_score.map(f32::to_bits),
            });
        let task_snapshot = BehaviorTaskSnapshot {
            execution_id: task.execution_id,
            current: capture_behavior_frame(
                &task.create_return_frame_after_action(task.action),
                documents,
            )?,
            target: task.target.map(reference).transpose()?,
            object: task.object.map(reference).transpose()?,
            next_action_tick: task.next_action_tick,
            interaction_slot: task.interaction_slot,
            stack: continuations
                .frames(actor.entity)
                .map(|frame| capture_behavior_frame(frame, documents))
                .collect::<Result<_, _>>()?,
            score,
            failing: actor.pending_behavior_task_failure.is_some(),
        };
        let navigation = capture_owned_navigation(&actor, task, entities, documents)?;
        let water_target = actor
            .selected_natural_water_target
            .map(|target| -> Result<_, WorldSnapshotPersistenceFailure> {
                Ok(BehaviorNaturalWaterTargetSnapshot {
                    region: reference(target.region)?,
                    water: target.water.to_array(),
                    approach: target.approach.to_array(),
                    depth: match target.kind {
                        NaturalWaterTargetKind::Shore => None,
                        NaturalWaterTargetKind::Surface { depth } => Some(depth),
                    },
                })
            })
            .transpose()?;
        let staff_job = actor
            .current_job
            .map(|current| {
                let job_entity = entities.get(current.job()).map_err(|_| {
                    WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference
                })?;
                let job = job_entity
                    .staff_job
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                let claim = job_entity
                    .job_claim
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                let behavior = job_entity
                    .staff_job_behavior_task
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                if claim.staff != actor.entity || behavior.execution_id != Some(task.execution_id) {
                    return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
                }
                let routing = job_entity.staff_job_routing.copied().unwrap_or_default();
                Ok(BehaviorStaffJobSnapshot {
                    subject_type: routing.subject_type,
                    recipient: routing.recipient,
                    kind: job.kind,
                    target: reference(job.target)?,
                    urgency: job.urgency,
                    token: job.token,
                    document: behavior
                        .document
                        .path()
                        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?
                        .to_string(),
                    program: behavior.task,
                    execution_id: task.execution_id,
                    progressing: job_entity.job_progress.is_some(),
                })
            })
            .transpose()?;
        records.push(BehaviorActorSnapshot::Task(BehaviorTaskActorSnapshot {
            persistent_identifier: id.0, task: task_snapshot, navigation, water_target, staff_job,
            transaction: actor.pending_behavior_transaction.map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
                if pending.payment.is_some() {
                    return Err(WorldSnapshotPersistenceFailure::SnapshotTransactionInFlight);
                }
                if !actor.instruction.is_some_and(|instruction| pending.owns(task, instruction)) {
                    return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
                }
                Ok(super::behavior_snapshot_types::BehaviorTransactionSnapshot {
                    execution_id: task.execution_id,
                    stack_depth: task.depth,
                    origin: capture_behavior_frame(&task.create_return_frame_after_action(task.action), documents)?,
                    facility: reference(pending.facility)?,
                    transaction: pending.transaction,
                    cost_override: pending.cost_override.map(f32::to_bits),
                    next_transaction: pending.next_transaction,
                })
            }).transpose()?,
            membership: memberships.container_for_member(actor.entity).map(|(container, slot)| -> Result<_, WorldSnapshotPersistenceFailure> {
                let definition = entities.get(container).ok()
                    .and_then(|container| container.definition_id.copied())
                    .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
                Ok(super::behavior_snapshot_types::BehaviorInteractionMembershipSnapshot {
                    container: reference(container)?, definition: definition.0, slot,
                    admission_order: memberships.members_of_container(container).position(|member| member == actor.entity)
                        .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?,
                    docked: memberships.is_docked(container, actor.entity),
                })
            }).transpose()?,
            random: actor.behavior_random_stream.map(|random| random.0.to_raw()),
            synchronized: actor.synchronized_behavior_owner
                .map(|owner| -> Result<_, WorldSnapshotPersistenceFailure> {
                    let (state, partner) = owner.iter().next().and_then(|operation| synchronizations.get(operation).ok())
                        .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                    Ok(BehaviorSynchronizedSnapshot {
                        execution_id: state.execution_id, target_execution_id: state.target_execution_id,
                        target: reference(partner.0)?, origin: capture_behavior_frame(&state.origin, documents)?,
                        depth: state.depth, target_outcome: state.target_outcome,
                    })
                }).transpose()?,
        }));
    }
    Ok(records)
}

fn capture_owned_navigation(
    actor: &BehaviorSnapshotEntity<'_, '_>,
    task: &BehaviorTaskExecutionState,
    entities: &BehaviorSnapshotEntities,
    documents: &Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>,
) -> Result<Option<BehaviorOwnedNavigationSnapshot>, WorldSnapshotPersistenceFailure> {
    let Some(agent) = actor.nav_agent else {
        return Ok(None);
    };
    let reference = |entity| capture_behavior_entity_reference(entity, entities);
    let move_completion = actor
        .pending_behavior_move_completion
        .filter(|pending| {
            actor
                .instruction
                .is_some_and(|instruction| pending.owns(task, instruction))
        })
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorNavigationCompletionSnapshot {
                origin: capture_behavior_frame(
                    &task.create_return_frame_after_action(task.action),
                    documents,
                )?,
                stack_depth: task.depth,
                request_id: pending.request_id,
                target: None,
            })
        })
        .transpose()?;
    let docking_completion = actor
        .pending_behavior_docking_completion
        .filter(|pending| {
            actor
                .instruction
                .is_some_and(|instruction| pending.owns(task, instruction, Some(pending.target)))
        })
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorNavigationCompletionSnapshot {
                origin: capture_behavior_frame(
                    &task.create_return_frame_after_action(task.action),
                    documents,
                )?,
                stack_depth: task.depth,
                request_id: pending.request_id,
                target: Some(reference(pending.target)?),
            })
        })
        .transpose()?;
    let pursuit = actor
        .pending_behavior_pursuit
        .filter(|pending| {
            actor
                .instruction
                .is_some_and(|instruction| pending.owns(task, instruction))
        })
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorPursuitSnapshot {
                execution_id: task.execution_id,
                origin: capture_behavior_frame(
                    &task.create_return_frame_after_action(task.action),
                    documents,
                )?,
                stack_depth: task.depth,
                poll_remaining_seconds: actor.behavior_pursuit_poll.map_or(
                    0.0,
                    openzt2_zoo_state::animal_behavior::behavior_pursuit_types::BehaviorPursuitPoll::remaining_seconds,
                ),
                leg: pending.leg,
            })
        })
        .transpose()?;
    let follow = actor
        .following
        .filter(|state| {
            actor
                .instruction
                .is_some_and(|step| step.is_current(task) && state.instruction == step.id())
        })
        .map(|state| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(super::behavior_snapshot_types::BehaviorFollowSnapshot {
                target: reference(state.target)?,
                until_tick: state.until_tick,
                request_id: state.request_id,
            })
        })
        .transpose()?;
    let spline = actor
        .spline
        .filter(|state| {
            actor
                .instruction
                .is_some_and(|step| step.is_current(task) && state.instruction == step.id())
        })
        .map(|state| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(super::behavior_snapshot_types::BehaviorSplineDockSnapshot {
                target: reference(state.target)?,
                phase: state.phase,
                owns_disabled_collider: state.owns_disabled_collider,
            })
        })
        .transpose()?;
    let owns = |request| {
        move_completion
            .as_ref()
            .is_some_and(|pending| pending.request_id == request)
            || docking_completion
                .as_ref()
                .is_some_and(|pending| pending.request_id == request)
            || pursuit.as_ref().is_some_and(|pending| {
                pending
                    .leg
                    .navigation_request_ids()
                    .contains(&Some(request))
            })
            || follow
                .as_ref()
                .is_some_and(|pending| pending.request_id == request)
            || spline
                .as_ref()
                .is_some_and(|pending| pending.phase.request() == Some(request))
    };
    let destination = actor
        .destination
        .filter(|destination| owns(destination.request_id))
        .copied();
    let docking = actor
        .docking
        .filter(|docking| owns(docking.request_id))
        .map(|docking| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorDockingSnapshot {
                request_id: docking.request_id,
                target: reference(docking.target)?,
                point: docking.point.to_array(),
                forward: docking.forward.to_array(),
                radius_m: docking.radius_m,
            })
        })
        .transpose()?;
    let pursuit_target = actor
        .entity_pursuit
        .filter(|pursuit| {
            destination.is_some_and(|destination| destination.request_id == pursuit.request_id)
        })
        .map(|pursuit| reference(pursuit.target))
        .transpose()?;
    let route = if destination.is_some() {
        actor
            .route
            .cloned()
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
    } else {
        Route::with_capacity(0)
    };
    Ok(Some(BehaviorOwnedNavigationSnapshot {
        agent: *agent,
        mode: actor.locomotion_mode.copied().unwrap_or_default(),
        swim_layer: actor.swim_layer.copied(),
        water_contact: actor.animal_terrain_water_contact.copied(),
        velocity: actor
            .linear_velocity
            .map(|velocity| Velocity(velocity.0))
            .or_else(|| actor.velocity.copied())
            .unwrap_or_default(),
        contact_steering: actor.contact_steering.copied(),
        progress: actor.motion_progress.copied(),
        destination,
        route,
        pursuit_target,
        docking,
        move_completion,
        docking_completion,
        pursuit,
        follow,
        spline,
        pending_results: Vec::new(),
    }))
}

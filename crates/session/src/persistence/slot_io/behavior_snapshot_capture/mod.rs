use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_zoo_state::{
    animal_behavior::behavior_natural_water_target_types::NaturalWaterTargetKind,
    behavior_task_execution_types::{BehaviorTaskExecutionState, BehaviorTaskReturnFrame},
    locomotion::locomotion_types::{Route, Velocity},
};

#[derive(SystemParam)]
pub(in crate::persistence) struct BehaviorSnapshotCapture<'w, 's> {
    entities: BehaviorSnapshotEntities<'w, 's>,
    arrivals: Res<'w, Messages<openzt2_zoo_state::locomotion::locomotion_types::Arrived>>,
    failures: Res<'w, Messages<openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed>>,
    occupancy: Res<'w, openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy>,
    documents: Res<'w, Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>>,
    finished: Res<'w, Messages<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished>>,
    failed: Res<'w, Messages<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed>>,
}

impl BehaviorSnapshotCapture<'_, '_> {
    pub(super) fn capture(
        &self,
    ) -> Result<Vec<BehaviorActorSnapshot>, WorldSnapshotPersistenceFailure> {
        let mut records =
            capture_behavior_actor_snapshots(&self.entities, &self.documents, &self.occupancy)?;
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
                let owner = self
                    .entities
                    .get(actor)
                    .ok()
                    .and_then(|actor| actor.synchronized_behavior_owner)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                if finished.iter().any(|event| {
                    event.actor == owner.target
                        && event.execution_id == synchronized.target_execution_id
                }) {
                    synchronized.target_outcome = Some(true);
                }
                if failed.iter().any(|event| {
                    event.actor == owner.target
                        && event.execution_id == synchronized.target_execution_id
                }) {
                    synchronized.target_outcome = Some(false);
                }
            }
        }
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
        BehaviorActorSnapshot, BehaviorDockingSnapshot, BehaviorFrameSnapshot,
        BehaviorNaturalWaterTargetSnapshot, BehaviorNavigationCompletionSnapshot,
        BehaviorOwnedNavigationSnapshot, BehaviorPursuitSnapshot, BehaviorStaffJobSnapshot,
        BehaviorSynchronizedSnapshot, BehaviorTaskScoreSnapshot, BehaviorTaskSnapshot,
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
            .filter(|set| set.id == frame.program)
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
    occupancy: &openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy,
) -> Result<Vec<BehaviorActorSnapshot>, WorldSnapshotPersistenceFailure> {
    let mut records = Vec::new();
    let reference = |entity| capture_behavior_entity_reference(entity, entities);
    for actor in entities {
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
            stack: task
                .stack
                .iter()
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
                let job_entity = entities.get(current.job).map_err(|_| {
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
                Ok(BehaviorStaffJobSnapshot {
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
        records.push(BehaviorActorSnapshot {
            persistent_identifier: id.0, task: task_snapshot, navigation, water_target, staff_job,
            membership: occupancy.container_for_member(actor.entity).map(|(container, slot)| -> Result<_, WorldSnapshotPersistenceFailure> {
                let definition = entities.get(container).ok()
                    .and_then(|container| container.definition_id.copied())
                    .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
                Ok(super::behavior_snapshot_types::BehaviorInteractionMembershipSnapshot {
                    container: reference(container)?, definition: definition.0, slot,
                    admission_order: occupancy.members_of_container(container).position(|member| member == actor.entity)
                        .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?,
                    docked: occupancy.is_docked(container, actor.entity),
                })
            }).transpose()?,
            random: actor.behavior_random_stream.map(|random| random.0.to_raw()),
            synchronized: actor.synchronized_behavior_owner
                .map(|owner| -> Result<_, WorldSnapshotPersistenceFailure> {
                    Ok(BehaviorSynchronizedSnapshot {
                        execution_id: owner.execution_id, target_execution_id: owner.target_execution_id,
                        target: reference(owner.target)?, origin: capture_behavior_frame(&owner.origin, documents)?,
                        depth: owner.depth, target_outcome: owner.target_outcome,
                    })
                }).transpose()?,
        });
    }
    records.sort_unstable_by_key(|actor| actor.persistent_identifier);
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
        .filter(|pending| pending.still_owns_exact_task_frame(task))
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorNavigationCompletionSnapshot {
                origin: capture_behavior_frame(&pending.origin, documents)?,
                stack_depth: pending.stack_depth,
                request_id: pending.request_id,
                target: None,
            })
        })
        .transpose()?;
    let docking_completion = actor
        .pending_behavior_docking_completion
        .filter(|pending| pending.still_owns_exact_task_frame(task))
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorNavigationCompletionSnapshot {
                origin: capture_behavior_frame(&pending.origin, documents)?,
                stack_depth: pending.stack_depth,
                request_id: pending.request_id,
                target: Some(reference(pending.target)?),
            })
        })
        .transpose()?;
    let pursuit = actor
        .pending_behavior_pursuit
        .filter(|pending| pending.still_owns_exact_task_frame(task))
        .map(|pending| -> Result<_, WorldSnapshotPersistenceFailure> {
            Ok(BehaviorPursuitSnapshot {
                execution_id: pending.execution_id,
                origin: capture_behavior_frame(&pending.origin, documents)?,
                stack_depth: pending.stack_depth,
                poll_remaining_seconds: pending.poll_remaining_seconds,
                leg: pending.leg,
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
        pending_results: Vec::new(),
    }))
}

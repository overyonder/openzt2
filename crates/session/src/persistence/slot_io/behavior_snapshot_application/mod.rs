mod definitions;
mod interaction_membership;
pub(super) mod parameters;
mod state_application;

use super::{
    behavior_snapshot_entity_references::index_behavior_snapshot_entity_references,
    behavior_snapshot_types::{BehaviorActorSnapshot, BehaviorSnapshotEntityReference},
};
use crate::persistence::persistence_failure_types::WorldSnapshotPersistenceFailure;
use bevy::prelude::*;
use definitions::{
    actor_references, prepare_behavior_snapshot_documents, resolve_behavior_frame,
    validate_behavior_snapshot_references,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_synchronized_set_types::SynchronizedBehaviorOwner,
        behavior_token_types::{BehaviorSelfToken, ClaimedBehaviorSelfToken},
    },
    behavior_task_execution_types::{BehaviorTaskExecutionPhase, SynchronizedBehaviorParticipant},
    staff::staff_job_types::TankCleaningRequestSite,
};
use parameters::{BehaviorSnapshotApplication, PreparedBehaviorActor};
use state_application::apply_prepared_behavior_actor;

pub(super) fn prepare_behavior_snapshot_before_world_replacement(
    records: &[BehaviorActorSnapshot],
    application: &BehaviorSnapshotApplication,
    retained: &mut Vec<Handle<BehaviorDocumentAsset>>,
    replaced_entities: &[Entity],
    imported_identifiers: impl Iterator<Item = u64>,
    topology_records: Option<&[openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot]>,
) -> Result<bool, WorldSnapshotPersistenceFailure> {
    if records.is_empty() {
        return Ok(true);
    }
    super::behavior_snapshot_section_encoding::validate_behavior_actor_snapshots(records)?;
    if !interaction_membership::validate_authored_behavior_memberships(records, application)? {
        return Ok(false);
    }
    if prepare_behavior_snapshot_documents(
        records,
        &application.documents,
        &application.server,
        retained,
    )?
    .is_none()
    {
        return Ok(false);
    }
    let capacity = application
        .capacity
        .as_deref()
        .filter(|capacity| capacity.is_valid());
    if records.iter().any(|record| record.navigation.is_some()) && capacity.is_none() {
        return Ok(false);
    }
    let mut identities = index_behavior_snapshot_entity_references(&application.entities)?;
    identities.retain(|reference, entity| {
        !replaced_entities.contains(entity)
            && !(topology_records.is_some()
                && matches!(reference, BehaviorSnapshotEntityReference::Habitat(_)))
    });
    for id in imported_identifiers {
        identities.insert(
            BehaviorSnapshotEntityReference::Persistent(id),
            Entity::PLACEHOLDER,
        );
    }
    if let Some(topology) = topology_records {
        for reference in
            super::behavior_snapshot_entity_references::planned_behavior_topology_references(
                topology,
            )?
        {
            identities.insert(reference, Entity::PLACEHOLDER);
        }
    }
    validate_behavior_snapshot_references(records, &identities)?;
    Ok(true)
}

pub(super) fn restore_behavior_actor_snapshots(
    commands: &mut Commands,
    records: &[BehaviorActorSnapshot],
    application: &mut BehaviorSnapshotApplication,
    retained: &mut Vec<Handle<BehaviorDocumentAsset>>,
) -> Result<bool, WorldSnapshotPersistenceFailure> {
    if records.is_empty() {
        return Ok(true);
    }
    super::behavior_snapshot_section_encoding::validate_behavior_actor_snapshots(records)?;
    if !interaction_membership::validate_authored_behavior_memberships(records, application)? {
        return Ok(false);
    }
    let Some(handles) = prepare_behavior_snapshot_documents(
        records,
        &application.documents,
        &application.server,
        retained,
    )?
    else {
        return Ok(false);
    };
    let mut identities = index_behavior_snapshot_entity_references(&application.entities)?;
    validate_behavior_snapshot_references(records, &identities)?;
    let capacity = application
        .capacity
        .as_deref()
        .filter(|capacity| capacity.is_valid());
    if records.iter().any(|record| record.navigation.is_some()) && capacity.is_none() {
        return Ok(false);
    }
    let mut prepared = Vec::with_capacity(records.len());
    for record in records {
        let resolve = |frame| resolve_behavior_frame(frame, &application.documents, &handles);
        let mut stack = arrayvec::ArrayVec::new();
        for frame in &record.task.stack {
            stack
                .try_push(resolve(frame)?)
                .map_err(|_| WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        }
        let nav = record.navigation.as_ref();
        if nav.is_some_and(|nav| nav.destination.is_none()
            && matches!(nav.pursuit.as_ref().map(|pursuit| pursuit.leg),
                Some(openzt2_zoo_state::animal_behavior::behavior_pursuit_types::BehaviorPursuitLeg::Route { .. })))
        {
            let target = record.task.target.as_ref()
                .and_then(|reference| identities.get(reference))
                .and_then(|target| application.entities.get(*target).ok())
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            if target.get::<GlobalTransform>().is_none() { return Ok(false); }
        }
        for reference in actor_references(record) {
            if let BehaviorSnapshotEntityReference::TankCleaningSite { tank, .. } = reference {
                let tank = identities[&BehaviorSnapshotEntityReference::Habitat(*tank)];
                if !application
                    .entities
                    .get(tank)
                    .is_ok_and(|tank| tank.contains::<WorldMember>())
                {
                    return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
                }
            }
        }
        if nav.is_some_and(|nav| {
            nav.route.points.len() > capacity.map_or(0, |capacity| capacity.max_route_points)
        }) {
            return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
        }
        let job_frame = record
            .staff_job
            .as_ref()
            .map(|job| {
                let mut frame = std::iter::once(&record.task.current)
                    .chain(&record.task.stack)
                    .find(|frame| frame.document == job.document && frame.program == job.program)
                    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
                    .clone();
                frame.phase = BehaviorTaskExecutionPhase::Execution;
                frame.action = 0;
                frame.repetitions = 0;
                resolve_behavior_frame(&frame, &application.documents, &handles)
            })
            .transpose()?;
        prepared.push(PreparedBehaviorActor {
            actor: identities
                [&BehaviorSnapshotEntityReference::Persistent(record.persistent_identifier)],
            current: resolve(&record.task.current)?,
            stack,
            move_origin: nav
                .and_then(|nav| nav.move_completion.as_ref())
                .map(|pending| resolve(&pending.origin))
                .transpose()?,
            dock_origin: nav
                .and_then(|nav| nav.docking_completion.as_ref())
                .map(|pending| resolve(&pending.origin))
                .transpose()?,
            pursuit_origin: nav
                .and_then(|nav| nav.pursuit.as_ref())
                .map(|pending| resolve(&pending.origin))
                .transpose()?,
            job_frame,
            synchronized_origin: record
                .synchronized
                .as_ref()
                .map(|owner| resolve(&owner.origin))
                .transpose()?,
        });
    }
    let restored_membership = interaction_membership::prepare_behavior_membership_restoration(
        records,
        &identities,
        application,
    )?;
    // Reserve the whole restored identity range before a request can allocate
    // a replacement leg, including identities belonging to later actor rows.
    for record in records {
        application
            .executions
            .reserve_restored(record.task.execution_id);
        if let Some(owner) = &record.synchronized {
            application
                .executions
                .reserve_restored(owner.target_execution_id);
        }
        if let Some(nav) = &record.navigation {
            for request in
                nav.move_completion
                    .iter()
                    .chain(&nav.docking_completion)
                    .map(|pending| pending.request_id)
                    .chain(nav.destination.map(|destination| destination.request_id))
                    .chain(nav.docking.iter().map(|dock| dock.request_id))
                    .chain(nav.pursuit.iter().flat_map(|pending| {
                        pending.leg.navigation_request_ids().into_iter().flatten()
                    }))
            {
                application.requests.reserve_restored(request);
            }
        }
    }
    // The complete plan is valid. Auxiliary task objects are created only now.
    for actor in &prepared {
        commands
            .entity(actor.actor)
            .remove::<(SynchronizedBehaviorOwner, SynchronizedBehaviorParticipant)>();
    }
    for record in records {
        for reference in actor_references(record) {
            if identities.contains_key(reference) {
                continue;
            }
            let entity = match reference {
                BehaviorSnapshotEntityReference::SelfToken {
                    recipient,
                    name,
                    available_at,
                    expires_at,
                } => {
                    let recipient =
                        identities[&BehaviorSnapshotEntityReference::Persistent(*recipient)];
                    commands
                        .spawn((
                            BehaviorSelfToken {
                                name: *name,
                                available_at: *available_at,
                                expires_at: *expires_at,
                            },
                            ClaimedBehaviorSelfToken {
                                execution_id: record.task.execution_id,
                            },
                            ChildOf(recipient),
                        ))
                        .id()
                }
                BehaviorSnapshotEntityReference::TankCleaningSite { tank, position } => {
                    let tank = identities[&BehaviorSnapshotEntityReference::Habitat(*tank)];
                    let world = application
                        .entities
                        .get(tank)
                        .ok()
                        .and_then(|tank| tank.get::<WorldMember>().copied())
                        .expect("validated tank world membership");
                    commands
                        .spawn((
                            TankCleaningRequestSite { tank },
                            world,
                            Transform::from_translation(Vec3::from_array(
                                position.map(f32::from_bits),
                            )),
                        ))
                        .id()
                }
                _ => unreachable!("all existing entity references were resolved"),
            };
            identities.insert(reference.clone(), entity);
        }
    }
    *application.occupancy = restored_membership;
    for (record, prepared) in records.iter().zip(prepared) {
        apply_prepared_behavior_actor(commands, record, prepared, &identities, application);
    }
    Ok(true)
}

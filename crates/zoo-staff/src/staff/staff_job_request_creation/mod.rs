use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_staff_requests::StaffControllerJobChange;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentIdAllocator, world_membership_types::WorldMember,
};

use super::{
    staff_job_types::{
        JobClaim, StaffJob, StaffJobCancellationPending, StaffJobRouting, StaffJobTarget,
        StaffJobsAtTarget, StaffRequestResourceCoverage,
    },
    staff_lifecycle_messages::StaffJobRequest,
};

type StaffJobRequestKey = (
    Entity,
    Option<u8>,
    Option<Entity>,
    Option<[u32; 3]>,
    Option<AssetId>,
    StaffJobRouting,
);

pub(crate) fn create_or_raise_priority_of_requested_staff_jobs(
    mut commands: Commands,
    mut requests: MessageReader<StaffJobRequest>,
    mut controller_changes: MessageReader<StaffControllerJobChange>,
    mut jobs: Query<(&mut StaffJob, Option<&StaffJobRouting>, Has<JobClaim>)>,
    targets: Query<(
        &WorldMember,
        Option<&StaffJobsAtTarget>,
        Option<&StaffRequestResourceCoverage>,
    )>,
    mut identifiers: Query<&mut PersistentIdAllocator>,
    mut batched_requests: Local<Vec<StaffJobRequest>>,
    mut request_indices: Local<HashMap<StaffJobRequestKey, usize>>,
) {
    request_indices.clear();
    for &request in requests.read() {
        if request.into_job().is_none() {
            continue;
        }
        if request.routing.subject_type.is_some() {
            if request.token.is_some() {
                batched_requests.push(request);
            }
            continue;
        }
        let key = (
            request.root,
            request.kind.map(|kind| kind as u8),
            request.target,
            request
                .point
                .map(|point| point.to_array().map(f32::to_bits)),
            request.token,
            request.routing,
        );
        let next = batched_requests.len();
        let index = *request_indices.entry(key).or_insert(next);
        if let Some(current) = batched_requests.get_mut(index) {
            current.priority = current.priority.min(request.priority);
        } else {
            batched_requests.push(request);
        }
    }

    let mut new_rows = HashMap::<(Entity, AssetId), Vec<Entity>>::default();
    let ordered = batched_requests
        .drain(..)
        .map(StaffControllerJobChange::Raise)
        .chain(controller_changes.read().copied());
    for change in ordered {
        let request = match change {
            StaffControllerJobChange::Raise(request) => request,
            StaffControllerJobChange::Cancel { target, token } => {
                let current = targets
                    .get(target)
                    .ok()
                    .and_then(|(_, existing, _)| existing)
                    .into_iter()
                    .flat_map(|jobs| jobs.iter())
                    .filter(|&entity| {
                        jobs.get(entity).is_ok_and(|(job, _, _)| {
                            job.token == Some(token) && job.point.is_none()
                        })
                    });
                let just_created = new_rows.remove(&(target, token)).unwrap_or_default();
                for entity in current.chain(just_created) {
                    commands
                        .entity(entity)
                        .insert_if_new(StaffJobCancellationPending);
                }
                continue;
            }
        };
        let Some(job) = request.into_job() else {
            continue;
        };
        let target = request.target.and_then(|target| {
            let (member, existing, coverage) = targets.get(target).ok()?;
            (member.root == request.root).then_some((target, existing, coverage))
        });
        if target.is_none() && request.point.is_none() {
            continue;
        }
        if target
            .and_then(|(_, _, coverage)| coverage)
            .is_some_and(|coverage| {
                (coverage.food && request.token == Some(AssetId::from_key("t_feedanimal")))
                    || (coverage.water && request.token == Some(AssetId::from_key("t_wateranimal")))
            })
        {
            continue;
        }
        let matching = target
            .and_then(|(_, existing, _)| existing)
            .filter(|_| request.routing.subject_type.is_none())
            .into_iter()
            .flat_map(|jobs| jobs.iter())
            .find(|&entity| {
                jobs.get(entity).is_ok_and(|(job, routing, _)| {
                    job.kind == request.kind
                        && job.point == request.point
                        && job.token == request.token
                        && routing.copied().unwrap_or_default() == request.routing
                })
            });
        if let Some((job, _, claimed)) = matching.and_then(|entity| jobs.get_mut(entity).ok()) {
            if !claimed && request.priority < job.priority {
                job.map_unchanged(|job| &mut job.priority)
                    .set_if_neq(request.priority);
            }
            continue;
        }
        let Ok(mut allocator) = identifiers.get_mut(request.root) else {
            continue;
        };
        let Ok(identifier) = allocator.allocate() else {
            continue;
        };
        let mut spawned = commands.spawn((
            job,
            request.routing,
            WorldMember { root: request.root },
            identifier,
        ));
        if let Some((target, _, _)) = target {
            spawned.insert(StaffJobTarget(target));
            if let Some(token) = request.token.filter(|_| request.point.is_none()) {
                new_rows
                    .entry((target, token))
                    .or_default()
                    .push(spawned.id());
            }
        }
    }
}

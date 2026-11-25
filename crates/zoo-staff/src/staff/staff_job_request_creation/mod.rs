use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentIdAllocator, world_membership_types::WorldMember,
};

use super::{
    staff_job_types::{JobClaim, StaffJob, StaffJobRouting},
    staff_lifecycle_messages::StaffJobRequest,
};

type StaffJobRequestKey = (Option<u8>, Entity, Option<AssetId>, StaffJobRouting);

pub(crate) fn create_or_raise_priority_of_requested_staff_jobs(
    mut commands: Commands,
    mut requests: MessageReader<StaffJobRequest>,
    mut jobs: Query<(
        Entity,
        &mut StaffJob,
        Option<&StaffJobRouting>,
        Has<JobClaim>,
    )>,
    members: Query<&WorldMember>,
    mut identifiers: Query<&mut PersistentIdAllocator>,
    mut batched_requests: Local<Vec<StaffJobRequest>>,
    mut request_indices: Local<HashMap<StaffJobRequestKey, usize>>,
) {
    // Preserve first-seen order when allocating identifiers for new jobs.
    request_indices.clear();
    for &request in requests.read() {
        let key = (
            request.kind.map(|kind| kind as u8),
            request.target,
            request.token,
            request.routing,
        );
        let next_index = batched_requests.len();
        let index = *request_indices.entry(key).or_insert(next_index);
        if let Some(current) = batched_requests.get_mut(index) {
            current.urgency = current.urgency.max(request.urgency);
        } else {
            batched_requests.push(request);
        }
    }

    if batched_requests.is_empty() {
        return;
    }
    let mut existing = vec![None; batched_requests.len()];
    for (entity, job, routing, _) in &jobs {
        let key = (
            job.kind.map(|kind| kind as u8),
            job.target,
            job.token,
            routing.copied().unwrap_or_default(),
        );
        if let Some(&index) = request_indices.get(&key) {
            let _ = existing[index].get_or_insert(entity);
        }
    }
    for (request, existing) in batched_requests.drain(..).zip(existing) {
        if let Some((_, job, _, claimed)) = existing.and_then(|entity| jobs.get_mut(entity).ok()) {
            if !claimed && request.urgency > job.urgency {
                job.map_unchanged(|job| &mut job.urgency)
                    .set_if_neq(request.urgency);
            }
            continue;
        }
        let Ok(member) = members.get(request.target) else {
            continue;
        };
        let Ok(mut allocator) = identifiers.get_mut(member.root) else {
            continue;
        };
        let Ok(identifier) = allocator.allocate() else {
            continue;
        };
        commands.spawn((
            StaffJob {
                kind: request.kind,
                target: request.target,
                urgency: request.urgency,
                token: request.token,
            },
            request.routing,
            *member,
            identifier,
        ));
    }
}

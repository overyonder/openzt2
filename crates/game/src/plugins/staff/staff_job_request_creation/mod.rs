use std::collections::HashMap;

use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use crate::plugins::world_spawn::{
    persistent_id_types::PersistentIdAllocator, world_membership_types::WorldMember,
};

use super::{
    staff_job_types::{JobClaim, StaffJob},
    staff_lifecycle_messages::StaffJobRequest,
};

/// Per-batch coalesced request: the first-seen (kind, target, token) in
/// message order and the maximum urgency seen for it in the same batch.
pub(crate) struct BatchedStaffJobRequest {
    kind: StaffJobKind,
    target: Entity,
    token: Option<AssetId>,
    urgency: u16,
}

pub(crate) fn create_or_raise_priority_of_requested_staff_jobs(
    mut commands: Commands,
    mut requests: MessageReader<StaffJobRequest>,
    jobs: Query<(Entity, &StaffJob, Option<&JobClaim>)>,
    members: Query<&WorldMember>,
    mut persistent_identifiers: ResMut<PersistentIdAllocator>,
    mut batched_request_maximums: Local<Vec<BatchedStaffJobRequest>>,
    mut batched_request_indices: Local<HashMap<(u8, Entity, Option<AssetId>), usize>>,
) {
    // Requests within one message batch coalesce per (kind, target, token)
    // before any job spawns, keeping first-seen message order so newly
    // spawned jobs allocate their persistent identifiers and claim in the
    // order the producers reported.  Both scratch collections are reused
    // across runs and cleared per frame, keeping storage bounded and
    // allocation-free once steady.
    batched_request_maximums.clear();
    batched_request_indices.clear();
    for request in requests.read() {
        let batch_key = (request.kind as u8, request.target, request.token);
        match batched_request_indices.get(&batch_key).copied() {
            Some(index) => {
                let batched_request = &mut batched_request_maximums[index];
                if request.urgency > batched_request.urgency {
                    batched_request.urgency = request.urgency;
                }
            }
            None => {
                batched_request_indices.insert(batch_key, batched_request_maximums.len());
                batched_request_maximums.push(BatchedStaffJobRequest {
                    kind: request.kind,
                    target: request.target,
                    token: request.token,
                    urgency: request.urgency,
                });
            }
        }
    }
    for batched_request in batched_request_maximums.drain(..) {
        let BatchedStaffJobRequest {
            kind,
            target,
            token,
            urgency,
        } = batched_request;
        if let Some((job_entity, job, claim)) = jobs
            .iter()
            .find(|(_, job, _)| job.kind == kind && job.target == target && job.token == token)
        {
            if claim.is_none() && urgency > job.urgency {
                commands
                    .entity(job_entity)
                    .insert(StaffJob { urgency, ..*job });
            }
            continue;
        }
        let Ok(world_membership) = members.get(target) else {
            continue;
        };
        let Ok(persistent_identifier) = persistent_identifiers.allocate(world_membership.root)
        else {
            continue;
        };
        commands.spawn((
            StaffJob {
                kind,
                target,
                urgency,
                token,
            },
            *world_membership,
            persistent_identifier,
        ));
    }
}

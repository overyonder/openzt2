use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    behavior_snapshot_types::{
        BehaviorActorSnapshot, BehaviorFrameSnapshot, BehaviorSnapshotEntityReference,
    },
};
use openzt2_zoo_state::behavior_task_execution_types::MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT;
use std::collections::{BTreeMap, BTreeSet};

const MAXIMUM_BEHAVIOR_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const MAXIMUM_BEHAVIOR_SNAPSHOT_ACTORS: usize = 65_536;
const MAXIMUM_BEHAVIOR_ROUTE_POINTS: usize = 65_536;

pub(super) fn encode_behavior_actor_snapshots(
    records: &[BehaviorActorSnapshot],
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    validate_behavior_actor_snapshots(records)?;
    let bytes = serde_json::to_vec(records)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if bytes.len() > MAXIMUM_BEHAVIOR_SNAPSHOT_BYTES {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_behavior_actor_snapshots(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<BehaviorActorSnapshot>, WorldSnapshotPersistenceFailure> {
    if section.payload_byte_count > MAXIMUM_BEHAVIOR_SNAPSHOT_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_BEHAVIOR_SNAPSHOT_ACTORS
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    // Earlier containers reserve this section without writing a task payload.
    if payload.is_empty() && section.encoded_record_count == 0 {
        return Ok(Vec::new());
    }
    let records: Vec<BehaviorActorSnapshot> = serde_json::from_slice(payload)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if records.len() != section.encoded_record_count as usize {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_behavior_actor_snapshots(&records)?;
    Ok(records)
}

fn valid_reference(reference: &BehaviorSnapshotEntityReference) -> bool {
    match reference {
        BehaviorSnapshotEntityReference::Persistent(id) => !matches!(*id, 0 | u64::MAX),
        BehaviorSnapshotEntityReference::TerrainWater { terrain, .. } => valid_asset_path(terrain),
        BehaviorSnapshotEntityReference::SelfToken {
            recipient,
            available_at,
            expires_at,
            ..
        } => !matches!(*recipient, 0 | u64::MAX) && available_at <= expires_at,
        BehaviorSnapshotEntityReference::TankCleaningSite { position, .. } => position
            .iter()
            .all(|value| f32::from_bits(*value).is_finite()),
        _ => true,
    }
}

fn valid_asset_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.split(['/', '\\']).any(|part| part == "..")
        && !path.starts_with('/')
        && bevy::asset::AssetPath::try_parse(path).is_ok()
}

fn valid_frame(frame: &BehaviorFrameSnapshot) -> bool {
    valid_asset_path(&frame.document)
        && frame.subjects.len() <= 4096
        && frame.subjects.windows(2).all(|pair| pair[0] < pair[1])
}

fn matches_frame(left: &BehaviorFrameSnapshot, right: &BehaviorFrameSnapshot) -> bool {
    left.document == right.document
        && left.program == right.program
        && left.subjects == right.subjects
        && left.phase == right.phase
        && left.action == right.action
}

pub(super) fn validate_behavior_actor_snapshots(
    records: &[BehaviorActorSnapshot],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() > MAXIMUM_BEHAVIOR_SNAPSHOT_ACTORS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let mut executions = BTreeSet::new();
    let mut request_owners = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let task = &record.task;
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || matches!(task.execution_id, 0 | u64::MAX)
            || !executions.insert(task.execution_id)
            || !valid_frame(&task.current)
            || task.stack.iter().any(|frame| !valid_frame(frame))
            || task.stack.len() > MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT
            || task
                .target
                .iter()
                .chain(&task.object)
                .any(|reference| !valid_reference(reference))
            || task
                .score
                .as_ref()
                .is_some_and(|score| score.execution_id != task.execution_id)
            || record.random.is_some_and(|random| random[1] & 1 == 0)
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        if matches!(&task.object, Some(BehaviorSnapshotEntityReference::SelfToken { recipient, .. })
            if *recipient != record.persistent_identifier)
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        if let Some(water) = &record.water_target {
            if !valid_reference(&water.region)
                || water
                    .water
                    .iter()
                    .chain(&water.approach)
                    .any(|value| !value.is_finite())
                || water
                    .depth
                    .is_some_and(|depth| !depth.is_finite() || depth < 0.0)
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        if record
            .membership
            .as_ref()
            .is_some_and(|membership| !valid_reference(&membership.container))
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        if let Some(job) = &record.staff_job {
            if job.execution_id != task.execution_id
                || !valid_reference(&job.target)
                || !valid_asset_path(&job.document)
                || !std::iter::once(&task.current)
                    .chain(&task.stack)
                    .any(|frame| frame.program == job.program && frame.document == job.document)
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        if let Some(synchronized) = &record.synchronized {
            let origin = if synchronized.depth == task.stack.len() {
                Some(&task.current)
            } else {
                task.stack.get(synchronized.depth)
            };
            if synchronized.execution_id != task.execution_id
                || matches!(synchronized.target_execution_id, 0 | u64::MAX)
                || !valid_reference(&synchronized.target)
                || !valid_frame(&synchronized.origin)
                || origin.is_none_or(|origin| !matches_frame(origin, &synchronized.origin))
                || task.target.as_ref() != Some(&synchronized.target)
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        let Some(nav) = &record.navigation else {
            continue;
        };
        if !nav.agent.is_valid()
            || !nav.velocity.0.is_finite()
            || nav.route.points.len() > MAXIMUM_BEHAVIOR_ROUTE_POINTS
            || nav.route.cursor > nav.route.points.len()
            || nav.route.points.iter().any(|point| !point.is_finite())
            || nav.contact_steering.is_some_and(|contact| {
                !contact.direction.is_finite()
                    || !contact.factor.is_finite()
                    || !contact.recovery_delay_s.is_finite()
            })
            || nav
                .progress
                .is_some_and(|progress| !progress.last_position.is_finite())
            || nav
                .water_contact
                .is_some_and(|contact| contact.depth_m.is_some_and(|depth| !depth.is_finite()))
            || nav.pending_results.len() > 16
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        let mut requests = Vec::new();
        for completion in nav.move_completion.iter().chain(&nav.docking_completion) {
            if completion.stack_depth != task.stack.len()
                || !valid_frame(&completion.origin)
                || !matches_frame(&completion.origin, &task.current)
                || matches!(completion.request_id, 0 | u64::MAX)
                || completion
                    .target
                    .as_ref()
                    .is_some_and(|target| !valid_reference(target))
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            requests.push(completion.request_id);
        }
        if nav
            .docking_completion
            .as_ref()
            .is_some_and(|pending| pending.target.is_none())
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        if let Some(pursuit) = &nav.pursuit {
            if pursuit.execution_id != task.execution_id
                || pursuit.stack_depth != task.stack.len()
                || !matches_frame(&pursuit.origin, &task.current)
                || !pursuit.poll_remaining_seconds.is_finite()
                || task.target.is_none()
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            for request in pursuit.leg.navigation_request_ids().into_iter().flatten() {
                if matches!(request, 0 | u64::MAX) {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
                requests.push(request);
            }
        }
        if let Some(destination) = nav.destination {
            if !requests.contains(&destination.request_id)
                || !destination.world.is_finite()
                || !destination.arrival_radius_m.is_finite()
                || destination.arrival_radius_m < 0.0
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        } else if nav.pursuit_target.is_some() || !nav.route.points.is_empty() {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        for &request in &requests {
            if request_owners
                .insert(request, record.persistent_identifier)
                .is_some_and(|owner| owner != record.persistent_identifier)
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        for result in &nav.pending_results {
            if !requests.contains(&result.request_id()) {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            if let super::behavior_snapshot_types::BehaviorNavigationResultSnapshot::Arrived {
                target,
                ..
            } = result
            {
                if target
                    .as_ref()
                    .is_some_and(|target| !valid_reference(target))
                {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
            }
        }
        if let Some(target) = &nav.pursuit_target {
            if !valid_reference(target)
                || task.target.as_ref() != Some(target)
                || nav
                    .pursuit
                    .as_ref()
                    .zip(nav.destination)
                    .is_none_or(|(pending, destination)| {
                        !pending.leg.is_direct_request(destination.request_id)
                    })
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        if let Some(dock) = &nav.docking {
            if !requests.contains(&dock.request_id)
                || !valid_reference(&dock.target)
                || dock
                    .point
                    .iter()
                    .chain(&dock.forward)
                    .any(|value| !value.is_finite())
                || !dock.radius_m.is_finite()
                || dock.radius_m < 0.0
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
    }
    Ok(())
}

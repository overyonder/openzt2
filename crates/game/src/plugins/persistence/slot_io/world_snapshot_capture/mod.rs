use bevy::prelude::Name;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
    world_snapshot_types::{WorldSnapshotRecord, MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT},
};

pub(super) fn capture_world_snapshot_record_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> Result<WorldSnapshotRecord, WorldSnapshotPersistenceFailure> {
    let (world_root, world_selection, optional_world_name) = world_snapshot_capture_queries
        .world_roots
        .single()
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let world_display_name = optional_world_name
        .map(Name::as_str)
        .map(str::trim)
        .filter(|world_display_name| {
            !world_display_name.is_empty()
                && world_display_name.len() <= MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT
                && !world_display_name.chars().any(char::is_control)
        })
        .unwrap_or("Zoo");

    Ok(WorldSnapshotRecord {
        scenario_definition_identifier: world_root.scenario,
        requested_world_definition_identifier: world_selection.requested,
        map_definition_identifier: world_selection.map,
        starting_point_definition_identifier: world_selection.start,
        session_mode: world_selection.mode,
        last_saved_unix_timestamp_milliseconds: current_unix_timestamp_milliseconds(),
        display_name: world_display_name.to_owned(),
    })
}

fn current_unix_timestamp_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

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

    let source_document = match world_snapshot_capture_queries.world_documents.single() {
        Ok((handles, selection)) => {
            let assets = world_snapshot_capture_queries
                .world_scenario_assets
                .as_deref()
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
            let server = world_snapshot_capture_queries
                .asset_server
                .as_deref()
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
            let campaign = world_snapshot_capture_queries
                .world_scenarios
                .as_deref()
                .and_then(|active| active.get(assets))
                .and_then(|view| view.campaign_for_scenario(selection.requested))
                .map(|(campaign, _)| campaign);
            Some(
                super::saved_world_descriptor_capture::capture_saved_world_descriptor(
                    handles, selection, campaign, assets, server,
                )?,
            )
        }
        Err(_) => None,
    };
    Ok(WorldSnapshotRecord {
        scenario_definition_identifier: world_root.scenario,
        requested_world_definition_identifier: world_selection.requested,
        map_definition_identifier: world_selection.map,
        starting_point_definition_identifier: world_selection.start,
        session_mode: world_selection.mode,
        last_saved_unix_timestamp_milliseconds: current_unix_timestamp_milliseconds(),
        display_name: world_display_name.to_owned(),
        source_document,
    })
}

fn current_unix_timestamp_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::world_snapshot_types::{
    WorldSnapshotRecord, MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT,
};

const WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT: usize = 77;

pub(super) fn append_encoded_world_snapshot_section(
    bytes: &mut Vec<u8>,
    world: &WorldSnapshotRecord,
) {
    bytes.extend_from_slice(&world.scenario_definition_identifier.0);
    bytes.extend_from_slice(&world.requested_world_definition_identifier.0);
    bytes.extend_from_slice(&world.map_definition_identifier.0);
    bytes.extend_from_slice(&world.starting_point_definition_identifier.0);
    bytes.push(match world.session_mode {
        WorldSessionMode::Freeform => 0,
        WorldSessionMode::Challenge => 1,
        WorldSessionMode::Campaign => 2,
    });
    bytes.extend_from_slice(&world.last_saved_unix_timestamp_milliseconds.to_le_bytes());
    bytes.extend_from_slice(&(world.display_name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(world.display_name.as_bytes());
}

pub(super) fn decode_and_validate_world_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<WorldSnapshotRecord, WorldSnapshotPersistenceFailure> {
    if range.encoded_record_count != 1 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    if payload.len() < WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut scenario = [0; 16];
    scenario.copy_from_slice(&payload[..16]);
    let mut requested = [0; 16];
    requested.copy_from_slice(&payload[16..32]);
    let mut map = [0; 16];
    map.copy_from_slice(&payload[32..48]);
    let mut start = [0; 16];
    start.copy_from_slice(&payload[48..64]);
    let mode = match payload[64] {
        0 => WorldSessionMode::Freeform,
        1 => WorldSessionMode::Challenge,
        2 => WorldSessionMode::Campaign,
        _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    };
    let last_saved_unix_ms = u64::from_le_bytes(
        payload[65..73]
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    );
    let name_len = u32::from_le_bytes(
        payload[73..77]
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    ) as usize;
    if name_len == 0
        || name_len > MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT
        || WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT
            .checked_add(name_len)
            .is_none_or(|expected| expected != payload.len())
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let display_name = std::str::from_utf8(&payload[WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT..])
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if display_name.chars().any(char::is_control) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(WorldSnapshotRecord {
        scenario_definition_identifier: AssetId(scenario),
        requested_world_definition_identifier: AssetId(requested),
        map_definition_identifier: AssetId(map),
        starting_point_definition_identifier: AssetId(start),
        session_mode: mode,
        last_saved_unix_timestamp_milliseconds: last_saved_unix_ms,
        display_name: display_name.to_owned(),
    })
}

use openzt2_game_assets::world_scenario::world_scenario_document_source_encoding::{
    decode_world_scenario_document_source, encode_world_scenario_document_source,
};
use openzt2_game_data::AssetId;

use openzt2_world_state::game_session_types::WorldSessionMode;

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
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let source = world
        .source_document
        .as_ref()
        .map(encode_world_scenario_document_source)
        .transpose()
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    bytes.extend_from_slice(&world.scenario_definition_identifier.0);
    bytes.extend_from_slice(&world.requested_world_definition_identifier.0);
    bytes.extend_from_slice(&world.map_definition_identifier.0);
    bytes.extend_from_slice(&world.starting_point_definition_identifier.0);
    bytes.push(
        (match world.session_mode {
            WorldSessionMode::Freeform => 0,
            WorldSessionMode::Challenge => 1,
            WorldSessionMode::Campaign => 2,
        }) | if source.is_some() { 0x80 } else { 0 },
    );
    bytes.extend_from_slice(&world.last_saved_unix_timestamp_milliseconds.to_le_bytes());
    bytes.extend_from_slice(&(world.display_name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(world.display_name.as_bytes());
    if let Some(source) = source {
        bytes.extend_from_slice(&source);
    }
    Ok(())
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
    let has_source_document = payload[64] & 0x80 != 0;
    let mode = match payload[64] & 0x7f {
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
    let name_end = WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT
        .checked_add(name_len)
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if name_len == 0
        || name_len > MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT
        || name_end > payload.len()
        || (!has_source_document && name_end != payload.len())
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let display_name =
        std::str::from_utf8(&payload[WORLD_SNAPSHOT_FIXED_HEADER_BYTE_COUNT..name_end])
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let source_document = has_source_document
        .then(|| decode_world_scenario_document_source(&payload[name_end..]))
        .transpose()
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
        source_document,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::world_scenario::world_scenario_document_source::WorldScenarioDocumentSource;
    use openzt2_game_data::world_scenario::WorldScenarioDocument;

    #[test]
    fn embedded_world_document_and_legacy_world_payloads_round_trip_and_reject_trailing_bytes() {
        for source_document in [
            None,
            Some(WorldScenarioDocumentSource {
                document: WorldScenarioDocument::default(),
                starting_zoos: Box::new([]),
                terrains: Box::new([]),
                scenes: Box::new([]),
                textures: Box::new([]),
            }),
        ] {
            let world = WorldSnapshotRecord {
                scenario_definition_identifier: AssetId([1; 16]),
                requested_world_definition_identifier: AssetId([2; 16]),
                map_definition_identifier: AssetId([3; 16]),
                starting_point_definition_identifier: AssetId([4; 16]),
                session_mode: WorldSessionMode::Challenge,
                last_saved_unix_timestamp_milliseconds: 1234,
                display_name: "Restored zoo".into(),
                source_document,
            };
            let mut bytes = Vec::new();
            append_encoded_world_snapshot_section(&mut bytes, &world).unwrap();
            let range = WorldSnapshotSectionDirectoryEntry {
                encoded_record_count: 1,
                payload_start_byte_offset: 0,
                payload_byte_count: bytes.len() as u64,
            };
            let decoded = decode_and_validate_world_snapshot_section(&bytes, range).unwrap();
            assert_eq!(decoded.display_name, world.display_name);
            assert_eq!(decoded.last_saved_unix_timestamp_milliseconds, 1234);
            assert_eq!(
                decoded.source_document.is_some(),
                world.source_document.is_some()
            );
            let mut encoded_again = Vec::new();
            append_encoded_world_snapshot_section(&mut encoded_again, &decoded).unwrap();
            assert_eq!(encoded_again, bytes);
            bytes.push(0);
            assert!(decode_and_validate_world_snapshot_section(
                &bytes,
                WorldSnapshotSectionDirectoryEntry {
                    payload_byte_count: bytes.len() as u64,
                    ..range
                }
            )
            .is_err());
        }
    }
}

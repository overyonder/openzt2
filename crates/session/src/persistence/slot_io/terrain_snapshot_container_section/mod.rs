use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    terrain_snapshot_section_encoding::decode_terrain_snapshot_record,
    terrain_snapshot_types::TerrainSnapshotRecord,
};

pub(super) fn decode_terrain_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<Option<TerrainSnapshotRecord>, Failure> {
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    match range.encoded_record_count {
        0 if payload.is_empty() => Ok(None),
        1 => decode_terrain_snapshot_record(payload).map(Some),
        _ => Err(Failure::CorruptSnapshotSection),
    }
}

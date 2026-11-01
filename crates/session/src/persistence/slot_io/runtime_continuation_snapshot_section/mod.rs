use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    runtime_continuation_snapshot_encoding::{
        append_runtime_continuation_snapshot, decode_runtime_continuation_snapshot,
    },
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
};

pub(super) fn encode_runtime_continuation_section(
    record: Option<&RuntimeContinuationSnapshot>,
) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    if let Some(record) = record {
        append_runtime_continuation_snapshot(&mut bytes, record)?;
    }
    Ok(bytes)
}

pub(super) fn decode_runtime_continuation_section(
    container: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Option<RuntimeContinuationSnapshot>, Failure> {
    let bytes = &container[section.payload_byte_range_within_container(container.len())?];
    match (section.encoded_record_count, bytes.is_empty()) {
        (0, true) => Ok(None),
        (1, false) => decode_runtime_continuation_snapshot(bytes).map(Some),
        _ => Err(Failure::CorruptSnapshotSection),
    }
}

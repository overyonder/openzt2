use bevy::prelude::Quat;

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    staff_snapshot_types::StaffSnapshotRecord,
};

const MAXIMUM_STAFF_SNAPSHOT_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_SAVED_STAFF_COUNT: usize = 65_536;

pub(super) fn encode_staff_snapshot_records(
    records: &[StaffSnapshotRecord],
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    validate_staff_snapshot_records(records)?;
    let bytes = serde_json::to_vec(records)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if bytes.len() > MAXIMUM_STAFF_SNAPSHOT_PAYLOAD_BYTES {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_staff_snapshot_records(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<StaffSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    if section.payload_byte_count > MAXIMUM_STAFF_SNAPSHOT_PAYLOAD_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_SAVED_STAFF_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    // Saves written before staff were stored carry an empty section.
    if section.encoded_record_count == 0 && section.payload_byte_count == 0 {
        return Ok(Vec::new());
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    let records: Vec<StaffSnapshotRecord> = serde_json::from_slice(payload)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if records.len() != section.encoded_record_count as usize {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_staff_snapshot_records(&records)?;
    Ok(records)
}

pub(super) fn validate_staff_snapshot_records(
    records: &[StaffSnapshotRecord],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() > MAXIMUM_SAVED_STAFF_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || record
                .translation
                .iter()
                .chain(&record.rotation)
                .chain(&record.scale)
                .any(|value| !value.is_finite())
            || (Quat::from_array(record.rotation).length_squared() - 1.0).abs() > 0.001
            || record.scale.iter().any(|value| *value <= 0.0)
            || record.wage_cents_per_month < 0
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

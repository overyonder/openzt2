use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    maintenance_snapshot_types::HabitatWasteSnapshotRecord,
};
use bevy::prelude::Quat;

pub(super) const MAXIMUM_WASTE_RECORDS: usize = 65_536;
const MAXIMUM_WASTE_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn validate_habitat_waste_records(
    records: &[HabitatWasteSnapshotRecord],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() > MAXIMUM_WASTE_RECORDS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || record.definition == openzt2_game_data::AssetId::default()
            || record.units == 0
            || record
                .translation
                .iter()
                .chain(&record.rotation)
                .chain(&record.scale)
                .any(|value| !value.is_finite())
            || (Quat::from_array(record.rotation).length_squared() - 1.0).abs() > 0.001
            || record.scale.iter().any(|value| *value <= 0.0)
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn encode_habitat_waste_records(
    records: &[HabitatWasteSnapshotRecord],
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    validate_habitat_waste_records(records)?;
    // Preserve the empty Maintenance section used by existing format-12 saves.
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = serde_json::to_vec(records)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if bytes.len() > MAXIMUM_WASTE_PAYLOAD_BYTES {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_habitat_waste_records(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<HabitatWasteSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    if section.payload_byte_count > MAXIMUM_WASTE_PAYLOAD_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_WASTE_RECORDS
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    if payload.is_empty() && section.encoded_record_count == 0 {
        return Ok(Vec::new());
    }
    let records: Vec<HabitatWasteSnapshotRecord> = serde_json::from_slice(payload)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if records.len() != section.encoded_record_count as usize {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_habitat_waste_records(&records)?;
    Ok(records)
}

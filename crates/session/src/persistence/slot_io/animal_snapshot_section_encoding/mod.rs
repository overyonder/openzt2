use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    animal_snapshot_types::AnimalSnapshotRecord,
};
use bevy::prelude::Quat;
use openzt2_game_data::species::Sex;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;

const MAXIMUM_ANIMAL_SNAPSHOT_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
const MAXIMUM_SAVED_ANIMAL_COUNT: usize = 65_536;

pub(super) fn encode_animal_snapshot_records(
    records: &[AnimalSnapshotRecord],
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    validate_animal_snapshot_records(records)?;
    let bytes = serde_json::to_vec(records)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if bytes.len() > MAXIMUM_ANIMAL_SNAPSHOT_PAYLOAD_BYTES {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_animal_snapshot_records(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<AnimalSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    if section.payload_byte_count > MAXIMUM_ANIMAL_SNAPSHOT_PAYLOAD_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_SAVED_ANIMAL_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    let records: Vec<AnimalSnapshotRecord> = serde_json::from_slice(payload)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if records.len() != section.encoded_record_count as usize {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_animal_snapshot_records(&records)?;
    Ok(records)
}

pub(super) fn validate_animal_snapshot_records(
    records: &[AnimalSnapshotRecord],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() > MAXIMUM_SAVED_ANIMAL_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || record.sex == Sex::Any
            || record
                .translation
                .iter()
                .chain(&record.rotation)
                .chain(&record.scale)
                .any(|value| !value.is_finite())
            || (Quat::from_array(record.rotation).length_squared() - 1.0).abs() > 0.001
            || record.scale.iter().any(|value| *value <= 0.0)
            || record.name.as_ref().is_some_and(|name| name.len() > 4096)
            || record.welfare.as_ref().is_some_and(|welfare| {
                welfare
                    .habitat_suitability
                    .iter()
                    .any(|value| *value > 1000)
                    || welfare
                        .needs
                        .iter()
                        .any(|value| !(0..=MAX_NEED_Q16).contains(value))
                    || welfare.triggers >> 11 != 0
                    || welfare.welfare > 1000
                    || welfare.band > 4
                    || welfare
                        .cumulative_points
                        .iter()
                        .any(|value| !value.is_finite())
            })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        for reference in record
            .parents
            .into_iter()
            .flatten()
            .chain(record.pregnancy.map(|(father, _)| father))
        {
            if reference == record.persistent_identifier
                || records
                    .binary_search_by_key(&reference, |animal| animal.persistent_identifier)
                    .is_err()
            {
                return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
            }
        }
    }
    Ok(())
}

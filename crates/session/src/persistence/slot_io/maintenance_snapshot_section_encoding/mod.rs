use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    maintenance_snapshot_types::{
        LooseWasteSnapshotRecord, MaintainedObjectSnapshotRecord, MaintenanceSnapshot,
    },
};
use bevy::prelude::Quat;

pub(super) const MAXIMUM_WASTE_RECORDS: usize = 65_536;
const MAXIMUM_WASTE_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn validate_loose_waste_records(
    records: &[LooseWasteSnapshotRecord],
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

fn validate_maintained_object_records(
    records: &[MaintainedObjectSnapshotRecord],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() > MAXIMUM_WASTE_RECORDS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || record
                .condition_permille
                .is_some_and(|condition| condition > 1_000)
            || record.condition_permille.is_none()
                && record.contained_waste.is_none()
                && record.next_waste_production_tick.is_none()
            || record
                .contained_waste
                .is_some_and(|[units, capacity]| capacity == 0 || units > capacity)
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn maintenance_record_count(snapshot: &MaintenanceSnapshot) -> usize {
    snapshot.waste.len() + snapshot.objects.len()
}

pub(super) fn encode_maintenance_snapshot(
    snapshot: &MaintenanceSnapshot,
) -> Result<Vec<u8>, WorldSnapshotPersistenceFailure> {
    validate_loose_waste_records(&snapshot.waste)?;
    validate_maintained_object_records(&snapshot.objects)?;
    if maintenance_record_count(snapshot) > MAXIMUM_WASTE_RECORDS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    // Preserve the empty Maintenance section used by existing format-12 saves.
    if maintenance_record_count(snapshot) == 0 {
        return Ok(Vec::new());
    }
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if bytes.len() > MAXIMUM_WASTE_PAYLOAD_BYTES {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_maintenance_snapshot(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<MaintenanceSnapshot, WorldSnapshotPersistenceFailure> {
    if section.payload_byte_count > MAXIMUM_WASTE_PAYLOAD_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_WASTE_RECORDS
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    if payload.is_empty() && section.encoded_record_count == 0 {
        return Ok(MaintenanceSnapshot::default());
    }
    let snapshot = serde_json::from_slice::<MaintenanceSnapshot>(payload)
        .or_else(|_| {
            serde_json::from_slice::<Vec<LooseWasteSnapshotRecord>>(payload).map(|waste| {
                MaintenanceSnapshot {
                    waste,
                    objects: Vec::new(),
                }
            })
        })
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if maintenance_record_count(&snapshot) != section.encoded_record_count as usize {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_loose_waste_records(&snapshot.waste)?;
    validate_maintained_object_records(&snapshot.objects)?;
    Ok(snapshot)
}

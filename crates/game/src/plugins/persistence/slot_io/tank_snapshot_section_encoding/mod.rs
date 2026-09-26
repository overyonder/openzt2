use openzt2_game_data::AssetId;

use crate::plugins::{
    aquatic::aquatic_simulation_types::{TankGeometry, WaterQuality},
    world_spawn::persistent_id_types::PersistentId,
};

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::tank_snapshot_types::TankSnapshotRecord;

const TANK_SNAPSHOT_RECORD_BYTE_COUNT: usize = 46;

pub(super) fn append_encoded_tank_snapshot_record(
    snapshot_section_bytes: &mut Vec<u8>,
    tank_snapshot_record: &TankSnapshotRecord,
) {
    snapshot_section_bytes
        .extend_from_slice(&tank_snapshot_record.persistent_identifier.0.to_le_bytes());
    snapshot_section_bytes.extend_from_slice(&tank_snapshot_record.tank_definition_identifier.0);
    [
        tank_snapshot_record.geometry.floor_height,
        tank_snapshot_record.geometry.wall_height,
        tank_snapshot_record.geometry.water_height,
        tank_snapshot_record.geometry.area,
        tank_snapshot_record.geometry.volume,
    ]
    .into_iter()
    .for_each(|geometry_value| {
        snapshot_section_bytes.extend_from_slice(&geometry_value.to_le_bytes());
    });
    snapshot_section_bytes.extend_from_slice(&tank_snapshot_record.water_quality.0.to_le_bytes());
}

pub(super) fn decode_and_validate_tank_snapshot_section(
    snapshot_container_bytes: &[u8],
    tank_section_byte_range: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<TankSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let tank_section_bytes = &snapshot_container_bytes[tank_section_byte_range
        .payload_byte_range_within_container(snapshot_container_bytes.len())?];
    if tank_section_bytes.len()
        != tank_section_byte_range.encoded_record_count as usize * TANK_SNAPSHOT_RECORD_BYTE_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    let mut tank_snapshot_records =
        Vec::with_capacity(tank_section_byte_range.encoded_record_count as usize);
    for encoded_tank_record in tank_section_bytes.chunks_exact(TANK_SNAPSHOT_RECORD_BYTE_COUNT) {
        let persistent_identifier = PersistentId(u64::from_le_bytes(
            encoded_tank_record[0..8]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        ));
        let mut tank_definition_identifier_bytes = [0; 16];
        tank_definition_identifier_bytes.copy_from_slice(&encoded_tank_record[8..24]);
        let decode_geometry_value = |byte_offset: usize| {
            encoded_tank_record[byte_offset..byte_offset + 4]
                .try_into()
                .map(f32::from_le_bytes)
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
        };
        let geometry = TankGeometry {
            floor_height: decode_geometry_value(24)?,
            wall_height: decode_geometry_value(28)?,
            water_height: decode_geometry_value(32)?,
            area: decode_geometry_value(36)?,
            volume: decode_geometry_value(40)?,
        };
        let water_quality = WaterQuality(u16::from_le_bytes(
            encoded_tank_record[44..46]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        ));
        if persistent_identifier.0 == 0
            || !geometry.is_valid()
            || water_quality.0 > 1_000
            || tank_snapshot_records
                .last()
                .is_some_and(|prior_record: &TankSnapshotRecord| {
                    prior_record.persistent_identifier.0 >= persistent_identifier.0
                })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        tank_snapshot_records.push(TankSnapshotRecord {
            persistent_identifier,
            tank_definition_identifier: AssetId(tank_definition_identifier_bytes),
            geometry,
            water_quality,
        });
    }
    Ok(tank_snapshot_records)
}

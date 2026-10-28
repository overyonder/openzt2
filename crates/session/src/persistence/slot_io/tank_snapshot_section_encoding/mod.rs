use bevy::math::IVec2;
use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::tank_snapshot_types::{
    LoadedTankSnapshotIdentity, LoadedTankSnapshotRecord, TankSnapshotRecord,
};

/// Older records: persistent identifier, definition, five geometry floats and
/// water quality.
const LEGACY_TANK_SNAPSHOT_RECORD_BYTE_COUNT: usize = 46;
/// Earlier seeded records omit the player water ceiling.
const SEEDED_TANK_SNAPSHOT_RECORD_BYTE_COUNT: usize = 47;
/// Current seeded records add the water ceiling; wall ground is derived on load.
const LIMITED_TANK_SNAPSHOT_RECORD_BYTE_COUNT: usize = 51;
const HABITAT_SEED_CELL_IDENTITY_TAG: u8 = 1;

pub(super) fn append_encoded_tank_snapshot_record(
    snapshot_section_bytes: &mut Vec<u8>,
    tank_snapshot_record: &TankSnapshotRecord,
) {
    snapshot_section_bytes.push(HABITAT_SEED_CELL_IDENTITY_TAG);
    snapshot_section_bytes.extend_from_slice(&tank_snapshot_record.seed_cell.x.to_le_bytes());
    snapshot_section_bytes.extend_from_slice(&tank_snapshot_record.seed_cell.y.to_le_bytes());
    snapshot_section_bytes.extend_from_slice(&tank_snapshot_record.tank_definition_identifier.0);
    let geometry = tank_snapshot_record.geometry;
    [
        geometry.floor_height,
        geometry.wall_height,
        geometry.water_height,
        geometry.area,
        geometry.volume,
        geometry.water_height_limit,
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
) -> Result<Vec<LoadedTankSnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let tank_section_bytes = &snapshot_container_bytes[tank_section_byte_range
        .payload_byte_range_within_container(snapshot_container_bytes.len())?];
    let record_count = tank_section_byte_range.encoded_record_count as usize;
    if record_count == 0 {
        return if tank_section_bytes.is_empty() {
            Ok(Vec::new())
        } else {
            Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
        };
    }
    let record_byte_count = [
        LIMITED_TANK_SNAPSHOT_RECORD_BYTE_COUNT,
        SEEDED_TANK_SNAPSHOT_RECORD_BYTE_COUNT,
        LEGACY_TANK_SNAPSHOT_RECORD_BYTE_COUNT,
    ]
    .into_iter()
    .find(|record_byte_count| {
        record_count.checked_mul(*record_byte_count) == Some(tank_section_bytes.len())
    })
    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;

    let corrupt = || WorldSnapshotPersistenceFailure::CorruptSnapshotSection;
    let mut tank_snapshot_records = Vec::with_capacity(record_count);
    for encoded_tank_record in tank_section_bytes.chunks_exact(record_byte_count) {
        let (identity, body) = if record_byte_count != LEGACY_TANK_SNAPSHOT_RECORD_BYTE_COUNT {
            if encoded_tank_record[0] != HABITAT_SEED_CELL_IDENTITY_TAG {
                return Err(corrupt());
            }
            let coordinate = |offset: usize| {
                encoded_tank_record[offset..offset + 4]
                    .try_into()
                    .map(i32::from_le_bytes)
                    .map_err(|_| corrupt())
            };
            (
                LoadedTankSnapshotIdentity::HabitatSeedCell(IVec2::new(
                    coordinate(1)?,
                    coordinate(5)?,
                )),
                &encoded_tank_record[9..],
            )
        } else {
            let persistent_identifier = PersistentId(u64::from_le_bytes(
                encoded_tank_record[0..8]
                    .try_into()
                    .map_err(|_| corrupt())?,
            ));
            if persistent_identifier.0 == 0 {
                return Err(corrupt());
            }
            (
                LoadedTankSnapshotIdentity::LegacyPersistentEntity(persistent_identifier),
                &encoded_tank_record[8..],
            )
        };
        let mut tank_definition_identifier_bytes = [0; 16];
        tank_definition_identifier_bytes.copy_from_slice(&body[0..16]);
        let decode_geometry_value = |byte_offset: usize| {
            body[byte_offset..byte_offset + 4]
                .try_into()
                .map(f32::from_le_bytes)
                .map_err(|_| corrupt())
        };
        let floor_height = decode_geometry_value(16)?;
        let geometry = TankGeometry {
            floor_height,
            wall_height: decode_geometry_value(20)?,
            water_height: decode_geometry_value(24)?,
            water_height_limit: if record_byte_count == LIMITED_TANK_SNAPSHOT_RECORD_BYTE_COUNT {
                decode_geometry_value(36)?
            } else {
                1_000_000.0
            },
            // Replaced from terrain under the walls once the tank is live.
            wall_base_height: floor_height,
            area: decode_geometry_value(28)?,
            volume: decode_geometry_value(32)?,
        };
        let quality_offset = if record_byte_count == LIMITED_TANK_SNAPSHOT_RECORD_BYTE_COUNT {
            40
        } else {
            36
        };
        let water_quality = WaterQuality(u16::from_le_bytes(
            body[quality_offset..quality_offset + 2]
                .try_into()
                .map_err(|_| corrupt())?,
        ));
        let ordered_after_previous =
            tank_snapshot_records
                .last()
                .is_none_or(
                    |prior: &LoadedTankSnapshotRecord| match (prior.identity, identity) {
                        (
                            LoadedTankSnapshotIdentity::HabitatSeedCell(prior),
                            LoadedTankSnapshotIdentity::HabitatSeedCell(next),
                        ) => (prior.x, prior.y) < (next.x, next.y),
                        (
                            LoadedTankSnapshotIdentity::LegacyPersistentEntity(prior),
                            LoadedTankSnapshotIdentity::LegacyPersistentEntity(next),
                        ) => prior.0 < next.0,
                        _ => false,
                    },
                );
        if !geometry.is_valid() || water_quality.0 > 1_000 || !ordered_after_previous {
            return Err(corrupt());
        }
        tank_snapshot_records.push(LoadedTankSnapshotRecord {
            identity,
            water_limit_missing: record_byte_count != LIMITED_TANK_SNAPSHOT_RECORD_BYTE_COUNT,
            tank_definition_identifier: AssetId(tank_definition_identifier_bytes),
            geometry,
            water_quality,
        });
    }
    Ok(tank_snapshot_records)
}

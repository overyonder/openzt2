use openzt2_game_data::AssetId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_section_encoding::decode_and_validate_progression_snapshot_section,
    simulation_time_snapshot_section::{
        decode_simulation_time_snapshot_record, SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT,
    },
    snapshot_container_encoding_and_validation::validate_world_snapshot_container_and_section_directory,
    snapshot_container_types::{ValidatedWorldSnapshotContainer, WorldSnapshotSectionKind},
};
use super::{
    animal_adoption_offer_inventory_snapshot_section::decode_and_validate_animal_adoption_offer_inventory_snapshot_section,
    economy_snapshot_section_encoding::decode_and_validate_economy_snapshot_section,
    photo_snapshot_section_encoding::decode_and_validate_photo_snapshot_section,
    show_snapshot_section_encoding::decode_and_validate_show_snapshot_section,
    tank_snapshot_section_encoding::decode_and_validate_tank_snapshot_section,
    transport_snapshot_section_encoding::decode_and_validate_transport_snapshot_section,
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
};

const SCENARIO_RULE_SNAPSHOT_RECORD_BYTE_COUNT: usize = 42;

pub(super) fn validate_supported_snapshot_container_and_every_section_payload(
    snapshot_container_bytes: &[u8],
    required_profile_identifier: AssetId,
) -> Result<ValidatedWorldSnapshotContainer, WorldSnapshotPersistenceFailure> {
    let validated_snapshot_container = validate_world_snapshot_container_and_section_directory(
        snapshot_container_bytes,
        required_profile_identifier,
    )?;
    let world_section =
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::World);
    for snapshot_section in WorldSnapshotSectionKind::ENCODING_ORDER.into_iter().skip(1) {
        let section_byte_range =
            validated_snapshot_container.section_directory_entry(snapshot_section);
        if snapshot_section == WorldSnapshotSectionKind::Animals {
            let section_payload_bytes = &snapshot_container_bytes[section_byte_range
                .payload_byte_range_within_container(snapshot_container_bytes.len())?];
            decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
                section_payload_bytes,
                section_byte_range.encoded_record_count,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::Aquatic {
            decode_and_validate_tank_snapshot_section(
                snapshot_container_bytes,
                section_byte_range,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::Economy {
            decode_and_validate_economy_snapshot_section(
                snapshot_container_bytes,
                section_byte_range,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::Scenario {
            if section_byte_range.payload_byte_count
                != u64::from(section_byte_range.encoded_record_count)
                    * SCENARIO_RULE_SNAPSHOT_RECORD_BYTE_COUNT as u64
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        } else if snapshot_section == WorldSnapshotSectionKind::Progression {
            let section_payload_bytes = &snapshot_container_bytes[section_byte_range
                .payload_byte_range_within_container(snapshot_container_bytes.len())?];
            decode_and_validate_progression_snapshot_section(
                section_payload_bytes,
                section_byte_range.encoded_record_count,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::SimulationTime {
            if section_byte_range.encoded_record_count != 1
                || section_byte_range.payload_byte_count
                    != SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            let section_payload_bytes = &snapshot_container_bytes[section_byte_range
                .payload_byte_range_within_container(snapshot_container_bytes.len())?];
            decode_simulation_time_snapshot_record(section_payload_bytes)?;
        } else if snapshot_section == WorldSnapshotSectionKind::Shows {
            decode_and_validate_show_snapshot_section(
                snapshot_container_bytes,
                section_byte_range,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::Transport {
            decode_and_validate_transport_snapshot_section(
                snapshot_container_bytes,
                section_byte_range,
            )?;
        } else if snapshot_section == WorldSnapshotSectionKind::Photos {
            decode_and_validate_photo_snapshot_section(
                snapshot_container_bytes,
                section_byte_range,
            )?;
        } else if section_byte_range.encoded_record_count != 0
            || section_byte_range.payload_byte_count != 0
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }

    decode_and_validate_world_snapshot_section(snapshot_container_bytes, world_section)?;
    Ok(validated_snapshot_container)
}

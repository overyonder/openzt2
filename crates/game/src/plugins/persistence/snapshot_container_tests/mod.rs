use openzt2_game_data::AssetId;

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_encoding_and_validation::{
        append_encoded_world_snapshot_section_to_container,
        finalize_world_snapshot_container_header_and_checksum,
        initialize_empty_world_snapshot_container,
        validate_world_snapshot_container_and_section_directory,
        WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION,
    },
    snapshot_container_types::WorldSnapshotSectionKind,
};

fn create_test_profile_identifier(repeated_identifier_byte: u8) -> AssetId {
    AssetId([repeated_identifier_byte; 16])
}

fn create_complete_test_world_snapshot_container(profile_identifier: AssetId) -> Vec<u8> {
    let mut snapshot_container_bytes = Vec::new();
    initialize_empty_world_snapshot_container(&mut snapshot_container_bytes, profile_identifier);
    for world_snapshot_section_kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        append_encoded_world_snapshot_section_to_container(
            &mut snapshot_container_bytes,
            world_snapshot_section_kind,
            1,
            |encoded_section_payload_bytes| {
                encoded_section_payload_bytes
                    .extend_from_slice(&(world_snapshot_section_kind as u16).to_le_bytes());
            },
        )
        .unwrap();
    }
    finalize_world_snapshot_container_header_and_checksum(&mut snapshot_container_bytes, 4)
        .unwrap();
    snapshot_container_bytes
}

#[test]
fn snapshot_bytes_are_deterministic_and_sections_are_exactly_ordered() {
    let first_encoded_snapshot_container =
        create_complete_test_world_snapshot_container(create_test_profile_identifier(1));
    let second_encoded_snapshot_container =
        create_complete_test_world_snapshot_container(create_test_profile_identifier(1));
    assert_eq!(
        first_encoded_snapshot_container,
        second_encoded_snapshot_container
    );
    let validated_snapshot_container = validate_world_snapshot_container_and_section_directory(
        &first_encoded_snapshot_container,
        create_test_profile_identifier(1),
    )
    .unwrap();
    assert_eq!(
        validated_snapshot_container.container_header.format_version,
        WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION
    );
    assert_eq!(
        validated_snapshot_container
            .container_header
            .section_directory_entry_count as usize,
        WorldSnapshotSectionKind::ENCODING_ORDER.len()
    );
    for world_snapshot_section_kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        let section_directory_entry =
            validated_snapshot_container.section_directory_entry(world_snapshot_section_kind);
        assert_eq!(section_directory_entry.encoded_record_count, 1);
        assert_eq!(
            &first_encoded_snapshot_container[section_directory_entry
                .payload_byte_range_within_container(first_encoded_snapshot_container.len())
                .unwrap()],
            &(world_snapshot_section_kind as u16).to_le_bytes()
        );
    }
}

#[test]
fn checksum_version_profile_and_ranges_reject_before_apply() {
    let original_snapshot_container_bytes =
        create_complete_test_world_snapshot_container(create_test_profile_identifier(2));

    let mut checksum_corrupted_snapshot_container_bytes = original_snapshot_container_bytes.clone();
    *checksum_corrupted_snapshot_container_bytes
        .last_mut()
        .unwrap() ^= 0x80;
    assert_eq!(
        validate_world_snapshot_container_and_section_directory(
            &checksum_corrupted_snapshot_container_bytes,
            create_test_profile_identifier(2)
        ),
        Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    );

    let mut future_version_snapshot_container_bytes = original_snapshot_container_bytes.clone();
    future_version_snapshot_container_bytes[8..12]
        .copy_from_slice(&(WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION + 1).to_le_bytes());
    assert_eq!(
        validate_world_snapshot_container_and_section_directory(
            &future_version_snapshot_container_bytes,
            create_test_profile_identifier(2)
        ),
        Err(WorldSnapshotPersistenceFailure::UnsupportedSnapshotFormatVersion)
    );
    assert_eq!(
        validate_world_snapshot_container_and_section_directory(
            &original_snapshot_container_bytes,
            create_test_profile_identifier(3)
        ),
        Err(WorldSnapshotPersistenceFailure::SnapshotProfileMismatch)
    );

    let mut invalid_section_range_snapshot_container_bytes = original_snapshot_container_bytes;
    let first_section_directory_entry_byte_offset = 76;
    invalid_section_range_snapshot_container_bytes[first_section_directory_entry_byte_offset + 8
        ..first_section_directory_entry_byte_offset + 16]
        .copy_from_slice(&u64::MAX.to_le_bytes());
    invalid_section_range_snapshot_container_bytes[44..76].fill(0);
    let mut blake3_hasher = blake3::Hasher::new();
    blake3_hasher.update(&invalid_section_range_snapshot_container_bytes[..44]);
    blake3_hasher.update(&invalid_section_range_snapshot_container_bytes[76..]);
    invalid_section_range_snapshot_container_bytes[44..76]
        .copy_from_slice(blake3_hasher.finalize().as_bytes());
    assert_eq!(
        validate_world_snapshot_container_and_section_directory(
            &invalid_section_range_snapshot_container_bytes,
            create_test_profile_identifier(2)
        ),
        Err(WorldSnapshotPersistenceFailure::CapacityExceeded)
    );
}

#[test]
fn truncated_directory_with_valid_checksum_is_rejected() {
    let profile = create_test_profile_identifier(4);
    let complete = create_complete_test_world_snapshot_container(profile);
    // Keep the header valid so rejection must come from the missing directory.
    for length in [76, 77, 99] {
        let mut truncated = complete[..length].to_vec();
        truncated[36..44].copy_from_slice(&((length - 76) as u64).to_le_bytes());
        let mut checksum = blake3::Hasher::new();
        checksum.update(&truncated[..44]);
        checksum.update(&truncated[76..]);
        truncated[44..76].copy_from_slice(checksum.finalize().as_bytes());
        assert_eq!(
            validate_world_snapshot_container_and_section_directory(&truncated, profile),
            Err(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader)
        );
    }
}

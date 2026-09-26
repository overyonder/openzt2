use std::array;

use openzt2_game_data::AssetId;

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::{
        ValidatedWorldSnapshotContainer, WorldSnapshotContainerHeader,
        WorldSnapshotSectionDirectoryEntry, WorldSnapshotSectionKind,
        WORLD_SNAPSHOT_SECTION_KIND_COUNT,
    },
};

pub(super) const WORLD_SNAPSHOT_CONTAINER_MAGIC_SIGNATURE_BYTES: [u8; 8] = *b"OZT2SAVE";
pub(super) const WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION: u32 = 11;
pub(super) const WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT: usize = 76;
pub(super) const WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT: usize = 24;
pub(super) const WORLD_SNAPSHOT_SECTION_COUNT: usize = WORLD_SNAPSHOT_SECTION_KIND_COUNT;
pub(super) const MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT: usize = 2 * 1024 * 1024 * 1024;
pub(super) const MAXIMUM_WORLD_SNAPSHOT_PERSISTENT_ENTITY_COUNT: u32 = 16_000_000;

pub(super) fn initialize_empty_world_snapshot_container(
    snapshot_container_bytes: &mut Vec<u8>,
    profile_identifier: AssetId,
) {
    snapshot_container_bytes.clear();
    snapshot_container_bytes.resize(
        WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT
            + WORLD_SNAPSHOT_SECTION_COUNT * WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT,
        0,
    );
    snapshot_container_bytes[0..8].copy_from_slice(&WORLD_SNAPSHOT_CONTAINER_MAGIC_SIGNATURE_BYTES);
    write_little_endian_u32_at_byte_offset(
        snapshot_container_bytes,
        8,
        WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION,
    );
    snapshot_container_bytes[12..28].copy_from_slice(&profile_identifier.0);
    write_little_endian_u32_at_byte_offset(
        snapshot_container_bytes,
        32,
        WORLD_SNAPSHOT_SECTION_COUNT as u32,
    );
}

pub(super) fn append_encoded_world_snapshot_section_to_container(
    snapshot_container_bytes: &mut Vec<u8>,
    snapshot_section_kind: WorldSnapshotSectionKind,
    encoded_record_count: u32,
    append_encoded_section_payload: impl FnOnce(&mut Vec<u8>),
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if snapshot_container_bytes.len()
        < WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT
            + WORLD_SNAPSHOT_SECTION_COUNT * WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader);
    }
    let section_directory_byte_offset = section_directory_byte_offset(snapshot_section_kind);
    if read_little_endian_u16_at_byte_offset(
        snapshot_container_bytes,
        section_directory_byte_offset,
    ) != 0
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let payload_start_byte_offset = snapshot_container_bytes.len();
    append_encoded_section_payload(snapshot_container_bytes);
    if snapshot_container_bytes.len() > MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT {
        snapshot_container_bytes.truncate(payload_start_byte_offset);
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let encoded_payload_byte_count = snapshot_container_bytes.len() - payload_start_byte_offset;
    write_little_endian_u16_at_byte_offset(
        snapshot_container_bytes,
        section_directory_byte_offset,
        snapshot_section_kind.wire_format_tag(),
    );
    write_little_endian_u32_at_byte_offset(
        snapshot_container_bytes,
        section_directory_byte_offset + 4,
        encoded_record_count,
    );
    write_little_endian_u64_at_byte_offset(
        snapshot_container_bytes,
        section_directory_byte_offset + 8,
        payload_start_byte_offset as u64,
    );
    write_little_endian_u64_at_byte_offset(
        snapshot_container_bytes,
        section_directory_byte_offset + 16,
        encoded_payload_byte_count as u64,
    );
    Ok(())
}

pub(super) fn finalize_world_snapshot_container_header_and_checksum(
    snapshot_container_bytes: &mut [u8],
    persistent_entity_count: u32,
) -> Result<WorldSnapshotContainerHeader, WorldSnapshotPersistenceFailure> {
    if persistent_entity_count > MAXIMUM_WORLD_SNAPSHOT_PERSISTENT_ENTITY_COUNT
        || snapshot_container_bytes.len() > MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    validate_world_snapshot_section_directory_shape(snapshot_container_bytes)?;
    let encoded_payload_byte_count = snapshot_container_bytes
        .len()
        .checked_sub(WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT)
        .ok_or(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader)?
        as u64;
    write_little_endian_u32_at_byte_offset(snapshot_container_bytes, 28, persistent_entity_count);
    write_little_endian_u64_at_byte_offset(
        snapshot_container_bytes,
        36,
        encoded_payload_byte_count,
    );
    snapshot_container_bytes[44..76].fill(0);
    let blake3_checksum =
        calculate_world_snapshot_container_blake3_checksum(snapshot_container_bytes);
    snapshot_container_bytes[44..76].copy_from_slice(&blake3_checksum);
    parse_world_snapshot_container_header(snapshot_container_bytes)
}

pub(super) fn validate_world_snapshot_container_and_section_directory(
    snapshot_container_bytes: &[u8],
    required_profile_identifier: AssetId,
) -> Result<ValidatedWorldSnapshotContainer, WorldSnapshotPersistenceFailure> {
    if snapshot_container_bytes.len() > MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let world_snapshot_container_header =
        parse_world_snapshot_container_header(snapshot_container_bytes)?;
    if world_snapshot_container_header.magic_signature_bytes
        != WORLD_SNAPSHOT_CONTAINER_MAGIC_SIGNATURE_BYTES
    {
        return Err(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader);
    }
    if world_snapshot_container_header.format_version != WORLD_SNAPSHOT_CONTAINER_FORMAT_VERSION {
        return Err(WorldSnapshotPersistenceFailure::UnsupportedSnapshotFormatVersion);
    }
    if world_snapshot_container_header.profile_identifier != required_profile_identifier {
        return Err(WorldSnapshotPersistenceFailure::SnapshotProfileMismatch);
    }
    if world_snapshot_container_header.persistent_entity_count
        > MAXIMUM_WORLD_SNAPSHOT_PERSISTENT_ENTITY_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    if world_snapshot_container_header.section_directory_entry_count
        != WORLD_SNAPSHOT_SECTION_COUNT as u32
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let required_encoded_payload_byte_count = snapshot_container_bytes
        .len()
        .checked_sub(WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT)
        .ok_or(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader)?
        as u64;
    if world_snapshot_container_header.encoded_payload_byte_count
        != required_encoded_payload_byte_count
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    if calculate_world_snapshot_container_blake3_checksum(snapshot_container_bytes)
        != world_snapshot_container_header.blake3_checksum
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    validate_world_snapshot_section_directory_shape(snapshot_container_bytes)?;
    let mut section_directory_entries =
        array::from_fn(|_| WorldSnapshotSectionDirectoryEntry::default());
    let first_section_payload_byte_offset = WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT
        .checked_add(
            WORLD_SNAPSHOT_SECTION_COUNT * WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    let mut previous_section_payload_end_byte_offset = first_section_payload_byte_offset;
    let mut total_encoded_record_count = 0_u64;
    for expected_snapshot_section_kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        let byte_offset = section_directory_byte_offset(expected_snapshot_section_kind);
        if read_little_endian_u16_at_byte_offset(snapshot_container_bytes, byte_offset)
            != expected_snapshot_section_kind.wire_format_tag()
            || read_little_endian_u16_at_byte_offset(snapshot_container_bytes, byte_offset + 2) != 0
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        let section_directory_entry = WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: read_little_endian_u32_at_byte_offset(
                snapshot_container_bytes,
                byte_offset + 4,
            ),
            payload_start_byte_offset: read_little_endian_u64_at_byte_offset(
                snapshot_container_bytes,
                byte_offset + 8,
            ),
            payload_byte_count: read_little_endian_u64_at_byte_offset(
                snapshot_container_bytes,
                byte_offset + 16,
            ),
        };
        let section_payload_byte_range = section_directory_entry
            .payload_byte_range_within_container(snapshot_container_bytes.len())?;
        if section_payload_byte_range.start != previous_section_payload_end_byte_offset {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        previous_section_payload_end_byte_offset = section_payload_byte_range.end;
        total_encoded_record_count = total_encoded_record_count
            .checked_add(u64::from(section_directory_entry.encoded_record_count))
            .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        section_directory_entries[expected_snapshot_section_kind.section_directory_index()] =
            section_directory_entry;
    }
    if previous_section_payload_end_byte_offset != snapshot_container_bytes.len()
        || total_encoded_record_count
            < u64::from(world_snapshot_container_header.persistent_entity_count)
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(ValidatedWorldSnapshotContainer {
        container_header: world_snapshot_container_header,
        section_directory_entries,
    })
}

fn calculate_world_snapshot_container_blake3_checksum(snapshot_container_bytes: &[u8]) -> [u8; 32] {
    let mut blake3_hasher = blake3::Hasher::new();
    blake3_hasher.update(&snapshot_container_bytes[..44]);
    blake3_hasher.update(&snapshot_container_bytes[WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT..]);
    *blake3_hasher.finalize().as_bytes()
}

fn validate_world_snapshot_section_directory_shape(
    snapshot_container_bytes: &[u8],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let section_directory_end_byte_offset = WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT
        .checked_add(
            WORLD_SNAPSHOT_SECTION_COUNT * WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if snapshot_container_bytes.len() < section_directory_end_byte_offset {
        return Err(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader);
    }
    for snapshot_section_kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        if read_little_endian_u16_at_byte_offset(
            snapshot_container_bytes,
            section_directory_byte_offset(snapshot_section_kind),
        ) != snapshot_section_kind.wire_format_tag()
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

fn parse_world_snapshot_container_header(
    snapshot_container_bytes: &[u8],
) -> Result<WorldSnapshotContainerHeader, WorldSnapshotPersistenceFailure> {
    if snapshot_container_bytes.len() < WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::InvalidSnapshotHeader);
    }
    let mut magic_signature_bytes = [0; 8];
    magic_signature_bytes.copy_from_slice(&snapshot_container_bytes[0..8]);
    let mut profile_identifier = [0; 16];
    profile_identifier.copy_from_slice(&snapshot_container_bytes[12..28]);
    let mut blake3_checksum = [0; 32];
    blake3_checksum.copy_from_slice(&snapshot_container_bytes[44..76]);
    Ok(WorldSnapshotContainerHeader {
        magic_signature_bytes,
        format_version: read_little_endian_u32_at_byte_offset(snapshot_container_bytes, 8),
        profile_identifier: AssetId(profile_identifier),
        persistent_entity_count: read_little_endian_u32_at_byte_offset(
            snapshot_container_bytes,
            28,
        ),
        section_directory_entry_count: read_little_endian_u32_at_byte_offset(
            snapshot_container_bytes,
            32,
        ),
        encoded_payload_byte_count: read_little_endian_u64_at_byte_offset(
            snapshot_container_bytes,
            36,
        ),
        blake3_checksum,
    })
}

const fn section_directory_byte_offset(snapshot_section_kind: WorldSnapshotSectionKind) -> usize {
    WORLD_SNAPSHOT_CONTAINER_HEADER_BYTE_COUNT
        + snapshot_section_kind.section_directory_index()
            * WORLD_SNAPSHOT_SECTION_DIRECTORY_ENTRY_BYTE_COUNT
}

fn read_little_endian_u16_at_byte_offset(
    snapshot_container_bytes: &[u8],
    byte_offset: usize,
) -> u16 {
    u16::from_le_bytes([
        snapshot_container_bytes[byte_offset],
        snapshot_container_bytes[byte_offset + 1],
    ])
}

fn read_little_endian_u32_at_byte_offset(
    snapshot_container_bytes: &[u8],
    byte_offset: usize,
) -> u32 {
    u32::from_le_bytes(
        snapshot_container_bytes[byte_offset..byte_offset + 4]
            .try_into()
            .unwrap_or([0; 4]),
    )
}

fn read_little_endian_u64_at_byte_offset(
    snapshot_container_bytes: &[u8],
    byte_offset: usize,
) -> u64 {
    u64::from_le_bytes(
        snapshot_container_bytes[byte_offset..byte_offset + 8]
            .try_into()
            .unwrap_or([0; 8]),
    )
}

fn write_little_endian_u16_at_byte_offset(
    snapshot_container_bytes: &mut [u8],
    byte_offset: usize,
    integer_value: u16,
) {
    snapshot_container_bytes[byte_offset..byte_offset + 2]
        .copy_from_slice(&integer_value.to_le_bytes());
}

fn write_little_endian_u32_at_byte_offset(
    snapshot_container_bytes: &mut [u8],
    byte_offset: usize,
    integer_value: u32,
) {
    snapshot_container_bytes[byte_offset..byte_offset + 4]
        .copy_from_slice(&integer_value.to_le_bytes());
}

fn write_little_endian_u64_at_byte_offset(
    snapshot_container_bytes: &mut [u8],
    byte_offset: usize,
    integer_value: u64,
) {
    snapshot_container_bytes[byte_offset..byte_offset + 8]
        .copy_from_slice(&integer_value.to_le_bytes());
}

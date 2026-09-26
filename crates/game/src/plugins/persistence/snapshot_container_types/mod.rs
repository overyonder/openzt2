//! World-snapshot container wire data and reusable loading storage.

use std::ops::Range;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::persistence_failure_types::WorldSnapshotPersistenceFailure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub(super) struct WorldSnapshotContainerHeader {
    pub(super) magic_signature_bytes: [u8; 8],
    pub(super) format_version: u32,
    pub(super) profile_identifier: AssetId,
    pub(super) persistent_entity_count: u32,
    pub(super) section_directory_entry_count: u32,
    pub(super) encoded_payload_byte_count: u64,
    pub(super) blake3_checksum: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub(super) enum WorldSnapshotSectionKind {
    World = 1,
    Terrain,
    Topology,
    Placement,
    Maintenance,
    Animals,
    Welfare,
    Behavior,
    Locomotion,
    Feeding,
    Health,
    Guests,
    Economy,
    Donations,
    Staff,
    Progression,
    Scenario,
    Aquatic,
    Shows,
    Transport,
    Photos,
    ExtinctAnimals,
    GesturesPuzzles,
    SimulationTime,
    Environment,
}

impl WorldSnapshotSectionKind {
    pub(super) const ENCODING_ORDER: [Self; 25] = [
        Self::World,
        Self::Terrain,
        Self::Topology,
        Self::Placement,
        Self::Maintenance,
        Self::Animals,
        Self::Welfare,
        Self::Behavior,
        Self::Locomotion,
        Self::Feeding,
        Self::Health,
        Self::Guests,
        Self::Economy,
        Self::Donations,
        Self::Staff,
        Self::Progression,
        Self::Scenario,
        Self::Aquatic,
        Self::Shows,
        Self::Transport,
        Self::Photos,
        Self::ExtinctAnimals,
        Self::GesturesPuzzles,
        Self::SimulationTime,
        Self::Environment,
    ];

    pub(super) const fn section_directory_index(self) -> usize {
        self as usize - 1
    }

    pub(super) const fn wire_format_tag(self) -> u16 {
        self as u16
    }
}

pub(super) const WORLD_SNAPSHOT_SECTION_KIND_COUNT: usize =
    WorldSnapshotSectionKind::ENCODING_ORDER.len();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct WorldSnapshotSectionDirectoryEntry {
    pub(super) encoded_record_count: u32,
    pub(super) payload_start_byte_offset: u64,
    pub(super) payload_byte_count: u64,
}

impl WorldSnapshotSectionDirectoryEntry {
    pub(super) fn payload_byte_range_within_container(
        self,
        snapshot_container_byte_count: usize,
    ) -> Result<Range<usize>, WorldSnapshotPersistenceFailure> {
        let payload_start_byte_offset = usize::try_from(self.payload_start_byte_offset)
            .map_err(|_| WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        let payload_byte_count = usize::try_from(self.payload_byte_count)
            .map_err(|_| WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        let payload_end_byte_offset = payload_start_byte_offset
            .checked_add(payload_byte_count)
            .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        if payload_end_byte_offset > snapshot_container_byte_count {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        Ok(payload_start_byte_offset..payload_end_byte_offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ValidatedWorldSnapshotContainer {
    pub(super) container_header: WorldSnapshotContainerHeader,
    pub(super) section_directory_entries:
        [WorldSnapshotSectionDirectoryEntry; WORLD_SNAPSHOT_SECTION_KIND_COUNT],
}

impl ValidatedWorldSnapshotContainer {
    pub(super) fn section_directory_entry(
        &self,
        snapshot_section_kind: WorldSnapshotSectionKind,
    ) -> WorldSnapshotSectionDirectoryEntry {
        self.section_directory_entries[snapshot_section_kind.section_directory_index()]
    }
}

#[derive(Resource, Debug, Default)]
pub(super) struct ReusableWorldSnapshotByteBuffer {
    pub(super) snapshot_container_bytes: Vec<u8>,
    pub(super) contains_validated_snapshot_container: bool,
}

impl ReusableWorldSnapshotByteBuffer {
    pub(super) fn clear_logical_contents(&mut self) {
        self.snapshot_container_bytes.clear();
        self.contains_validated_snapshot_container = false;
    }
}

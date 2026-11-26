use super::fence_topology_snapshot_section::SavedTopology;
use bevy::prelude::Component;
use openzt2_game_data::AssetId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    progression_snapshot_section_encoding::decode_and_validate_progression_snapshot_section,
    progression_snapshot_types::ProgressionSnapshotRecords,
    simulation_time_snapshot_section::{
        decode_simulation_time_snapshot_record, SimulationTimeSnapshotRecord,
    },
    snapshot_container_encoding_and_validation::validate_world_snapshot_container_and_section_directory,
    snapshot_container_types::WorldSnapshotSectionKind,
};
use super::{
    animal_adoption_offer_inventory_snapshot_section::{
        decode_and_validate_animal_adoption_offer_inventory_snapshot_section,
        AnimalAdoptionOffersSnapshot,
    },
    animal_snapshot_types::AnimalSnapshotRecord,
    behavior_snapshot_types::BehaviorActorSnapshot,
    donation_snapshot_section::DonationSnapshotRecord,
    economy_snapshot_section_encoding::decode_and_validate_economy_snapshot_section,
    economy_snapshot_types::EconomySnapshotRecords,
    maintenance_snapshot_types::MaintenanceSnapshot,
    photo_snapshot_section_encoding::decode_and_validate_photo_snapshot_section,
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    placement_snapshot_section::PlacementSnapshotRecord,
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
    scenario_snapshot_section::ScenarioSnapshot,
    show_snapshot_section_encoding::decode_and_validate_show_snapshot_section,
    show_snapshot_types::ShowSnapshotRecords,
    staff_snapshot_types::StaffSnapshotRecord,
    tank_snapshot_section_encoding::decode_and_validate_tank_snapshot_section,
    tank_snapshot_types::LoadedTankSnapshotRecord,
    terrain_snapshot_types::TerrainSnapshotRecord,
    transport_snapshot_section_encoding::decode_and_validate_transport_snapshot_section,
    transport_snapshot_types::TransportSnapshotRecords,
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
    world_snapshot_types::WorldSnapshotRecord,
};

#[derive(Component)]
pub(super) struct DecodedSnapshot {
    pub(super) profile: AssetId,
    pub(super) world: WorldSnapshotRecord,
    pub(super) terrain: Option<TerrainSnapshotRecord>,
    pub(super) behavior: Vec<BehaviorActorSnapshot>,
    pub(super) sections: Option<SnapshotRestoreSections>,
}

pub(super) struct SnapshotRestoreSections {
    pub(super) runtime: Option<RuntimeContinuationSnapshot>,
    pub(super) topology: Option<SavedTopology>,
    pub(super) maintenance: MaintenanceSnapshot,
    pub(super) animals: Vec<AnimalSnapshotRecord>,
    pub(super) staff: Vec<StaffSnapshotRecord>,
    pub(super) adoption: Option<AnimalAdoptionOffersSnapshot>,
    pub(super) tanks: Vec<LoadedTankSnapshotRecord>,
    pub(super) placements: Option<Vec<PlacementSnapshotRecord>>,
    pub(super) donations: Vec<DonationSnapshotRecord>,
    pub(super) economy: Option<EconomySnapshotRecords>,
    pub(super) scenario: Option<ScenarioSnapshot>,
    pub(super) progression: ProgressionSnapshotRecords,
    pub(super) time: SimulationTimeSnapshotRecord,
    pub(super) shows: ShowSnapshotRecords,
    pub(super) transport: TransportSnapshotRecords,
    pub(super) photos: Vec<PhotoSnapshotRecord>,
    pub(super) photo_challenges: Vec<PhotoChallengeProgressSnapshotRecord>,
}

pub(super) fn decode_snapshot_for_world_restore(
    bytes: &[u8],
    profile: AssetId,
) -> Result<DecodedSnapshot, Failure> {
    use WorldSnapshotSectionKind as Section;
    let container = validate_world_snapshot_container_and_section_directory(bytes, profile)?;
    let entry = |kind| container.section_directory_entry(kind);
    let payload = |kind| {
        let section = entry(kind);
        Ok::<_, Failure>(&bytes[section.payload_byte_range_within_container(bytes.len())?])
    };
    let world = decode_and_validate_world_snapshot_section(bytes, entry(Section::World))?;
    let terrain = super::terrain_snapshot_container_section::decode_terrain_snapshot_section(
        bytes,
        entry(Section::Terrain),
    )?;
    if terrain.is_some() != world.source_document.is_some()
        || entry(Section::SimulationTime).encoded_record_count != 1
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let behavior = super::behavior_snapshot_section_encoding::decode_behavior_actor_snapshots(
        bytes,
        entry(Section::Behavior),
    )?;
    let (photos, photo_challenges) =
        decode_and_validate_photo_snapshot_section(bytes, entry(Section::Photos))?;
    let sections = SnapshotRestoreSections {
        runtime: super::runtime_continuation_snapshot_section::decode_runtime_continuation_section(
            bytes,
            entry(Section::RuntimeContinuation),
        )?,
        topology: super::fence_topology_snapshot_section::decode_fence_topology_records(
            bytes,
            entry(Section::Topology),
        )?,
        maintenance: super::maintenance_snapshot_section_encoding::decode_maintenance_snapshot(
            bytes,
            entry(Section::Maintenance),
        )?,
        animals: super::animal_snapshot_section_encoding::decode_animal_snapshot_records(
            bytes,
            entry(Section::Animals),
        )?,
        staff: super::staff_snapshot_section_encoding::decode_staff_snapshot_records(
            bytes,
            entry(Section::Staff),
        )?,
        adoption: decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
            payload(Section::AdoptionOffers)?,
            entry(Section::AdoptionOffers).encoded_record_count,
            world.session_mode,
        )?,
        tanks: decode_and_validate_tank_snapshot_section(bytes, entry(Section::Aquatic))?,
        placements: super::placement_snapshot_section::decode_placement_records(
            bytes,
            entry(Section::Placement),
        )?,
        donations: super::donation_snapshot_section::decode_donation_records(
            bytes,
            entry(Section::Donations),
        )?,
        economy: Some(decode_and_validate_economy_snapshot_section(
            bytes,
            entry(Section::Economy),
        )?),
        scenario: super::scenario_snapshot_section::decode(bytes, entry(Section::Scenario))?,
        progression: decode_and_validate_progression_snapshot_section(
            payload(Section::Progression)?,
            entry(Section::Progression).encoded_record_count,
        )?,
        time: decode_simulation_time_snapshot_record(payload(Section::SimulationTime)?)?,
        shows: decode_and_validate_show_snapshot_section(bytes, entry(Section::Shows))?,
        transport: decode_and_validate_transport_snapshot_section(
            bytes,
            entry(Section::Transport),
        )?,
        photos,
        photo_challenges,
    };
    super::native_crate_snapshot_links::validate_native_crate_links(
        &sections.animals,
        sections.placements.as_deref().unwrap_or_default(),
    )?;
    for kind in Section::ENCODING_ORDER {
        match kind {
            Section::World
            | Section::Terrain
            | Section::RuntimeContinuation
            | Section::Topology
            | Section::Maintenance
            | Section::Behavior
            | Section::Animals
            | Section::Staff
            | Section::AdoptionOffers
            | Section::Aquatic
            | Section::Placement
            | Section::Donations
            | Section::Economy
            | Section::Scenario
            | Section::Progression
            | Section::SimulationTime
            | Section::Shows
            | Section::Transport
            | Section::Photos => continue,
            _ => {}
        }
        if entry(kind).encoded_record_count != 0 || entry(kind).payload_byte_count != 0 {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    Ok(DecodedSnapshot {
        profile,
        world,
        terrain,
        behavior,
        sections: Some(sections),
    })
}

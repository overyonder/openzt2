use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_section_encoding::append_encoded_progression_snapshot_section,
    progression_snapshot_types::ProgressionSnapshotRecords,
    simulation_time_snapshot_section::SimulationTimeSnapshotRecord,
    snapshot_container_encoding_and_validation::{
        append_encoded_world_snapshot_section_to_container,
        finalize_world_snapshot_container_header_and_checksum,
    },
    snapshot_container_types::WorldSnapshotSectionKind,
};
use super::{
    animal_adoption_offer_inventory_snapshot_section::{
        append_encoded_animal_adoption_offer_inventory_snapshot_section,
        validate_animal_adoption_offer_inventory_presence_for_session_mode,
    },
    economy_snapshot_section_encoding::append_encoded_economy_snapshot_section,
    economy_snapshot_types::EconomySnapshotRecords,
    photo_snapshot_section_encoding::append_encoded_photo_snapshot_section,
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    show_snapshot_section_encoding::append_encoded_show_snapshot_section,
    show_snapshot_types::{
        ScheduledShowBreakSnapshotRecord, ScheduledShowSnapshotRecord, ShowStageSnapshotRecord,
    },
    tank_snapshot_section_encoding::append_encoded_tank_snapshot_record,
    tank_snapshot_types::TankSnapshotRecord,
    transport_snapshot_section_encoding::append_encoded_transport_snapshot_section,
    transport_snapshot_types::TransportSnapshotRecords,
    world_snapshot_section_encoding::append_encoded_world_snapshot_section,
    world_snapshot_types::WorldSnapshotRecord,
};

// Each section is borrowed separately while the container is assembled.
#[allow(clippy::too_many_arguments)]
pub(super) fn append_captured_world_snapshot_sections_and_finalize_container(
    snapshot_container_bytes: &mut Vec<u8>,
    world_snapshot_record: &WorldSnapshotRecord,
    scenario_snapshot: Option<&super::scenario_snapshot_section::ScenarioSnapshot>,
    animal_adoption_offer_inventory: Option<
        &openzt2_animals::animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory,
    >,
    animal_snapshot_records: &[super::animal_snapshot_types::AnimalSnapshotRecord],
    maintenance_snapshot: &super::maintenance_snapshot_types::MaintenanceSnapshot,
    staff_snapshot_records: &[super::staff_snapshot_types::StaffSnapshotRecord],
    tank_snapshot_records: &[TankSnapshotRecord],
    topology_snapshot_records: &[openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot],
    economy_snapshot_records: &EconomySnapshotRecords,
    donation_snapshot_records: &[super::donation_snapshot_section::DonationSnapshotRecord],
    placement_snapshot_records: &[super::placement_snapshot_section::PlacementSnapshotRecord],
    progression_snapshot_records: &ProgressionSnapshotRecords,
    simulation_time_snapshot_record: &SimulationTimeSnapshotRecord,
    show_stage_snapshot_records: &[ShowStageSnapshotRecord],
    scheduled_show_snapshot_records: &[ScheduledShowSnapshotRecord],
    scheduled_show_break_snapshot_records: &[ScheduledShowBreakSnapshotRecord],
    transport_snapshot_records: &TransportSnapshotRecords,
    photo_snapshot_records: &[PhotoSnapshotRecord],
    photo_challenge_progress_snapshot_records: &[PhotoChallengeProgressSnapshotRecord],
    behavior_snapshot_records: &[super::behavior_snapshot_types::BehaviorActorSnapshot],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    validate_animal_adoption_offer_inventory_presence_for_session_mode(
        animal_adoption_offer_inventory,
        world_snapshot_record.session_mode,
    )?;
    let placement_bytes =
        super::placement_snapshot_section::encode_placement_records(placement_snapshot_records)?;
    let behavior_bytes =
        super::behavior_snapshot_section_encoding::encode_behavior_actor_snapshots(
            behavior_snapshot_records,
        )?;
    let topology_bytes = super::fence_topology_snapshot_section::encode_fence_topology_records(
        topology_snapshot_records,
    )?;
    let scenario_bytes = super::scenario_snapshot_section::encode(scenario_snapshot)?;
    let donation_bytes =
        super::donation_snapshot_section::encode_donation_records(donation_snapshot_records)?;
    let animal_bytes = super::animal_snapshot_section_encoding::encode_animal_snapshot_records(
        animal_snapshot_records,
    )?;
    let waste_bytes = super::maintenance_snapshot_section_encoding::encode_maintenance_snapshot(
        maintenance_snapshot,
    )?;
    let staff_bytes = super::staff_snapshot_section_encoding::encode_staff_snapshot_records(
        staff_snapshot_records,
    )?;
    WorldSnapshotSectionKind::ENCODING_ORDER
        .into_iter()
        .try_for_each(|snapshot_section| match snapshot_section {
            WorldSnapshotSectionKind::Topology => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    topology_snapshot_records.len() as u32,
                    |bytes| bytes.extend_from_slice(&topology_bytes),
                )
            }
            WorldSnapshotSectionKind::World => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                1,
                |section_bytes| {
                    append_encoded_world_snapshot_section(section_bytes, world_snapshot_record);
                },
            ),
            WorldSnapshotSectionKind::Behavior => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    behavior_snapshot_records.len() as u32,
                    |bytes| bytes.extend_from_slice(&behavior_bytes),
                )
            }
            WorldSnapshotSectionKind::Maintenance => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    super::maintenance_snapshot_section_encoding::maintenance_record_count(
                        maintenance_snapshot,
                    ) as u32,
                    |bytes| bytes.extend_from_slice(&waste_bytes),
                )
            }
            WorldSnapshotSectionKind::Animals => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    animal_snapshot_records.len() as u32,
                    |bytes| bytes.extend_from_slice(&animal_bytes),
                )
            }
            WorldSnapshotSectionKind::Staff => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                staff_snapshot_records.len() as u32,
                |bytes| bytes.extend_from_slice(&staff_bytes),
            ),
            WorldSnapshotSectionKind::AdoptionOffers => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    animal_adoption_offer_inventory
                        .map_or(0, |inventory| inventory.slot_count() as u32),
                    |section_bytes| {
                        append_encoded_animal_adoption_offer_inventory_snapshot_section(
                            section_bytes,
                            animal_adoption_offer_inventory,
                        );
                    },
                )
            }
            WorldSnapshotSectionKind::Aquatic => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    tank_snapshot_records.len() as u32,
                    |section_bytes| {
                        tank_snapshot_records
                            .iter()
                            .for_each(|tank_snapshot_record| {
                                append_encoded_tank_snapshot_record(
                                    section_bytes,
                                    tank_snapshot_record,
                                );
                            });
                    },
                )
            }
            WorldSnapshotSectionKind::Placement => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    placement_snapshot_records.len() as u32,
                    |bytes| bytes.extend_from_slice(&placement_bytes),
                )
            }
            WorldSnapshotSectionKind::Donations => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    donation_snapshot_records.len() as u32,
                    |bytes| bytes.extend_from_slice(&donation_bytes),
                )
            }
            WorldSnapshotSectionKind::Economy => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    1_u32
                        .saturating_add(
                            economy_snapshot_records.monthly_finance_records.len() as u32
                        )
                        .saturating_add(
                            economy_snapshot_records.facility_economy_records.len() as u32
                        ),
                    |section_bytes| {
                        append_encoded_economy_snapshot_section(
                            section_bytes,
                            economy_snapshot_records,
                        );
                    },
                )
            }
            WorldSnapshotSectionKind::Scenario => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    super::scenario_snapshot_section::record_count(scenario_snapshot),
                    |bytes| bytes.extend_from_slice(&scenario_bytes),
                )
            }
            WorldSnapshotSectionKind::Progression => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    progression_snapshot_records.snapshot_record_count(),
                    |section_bytes| {
                        append_encoded_progression_snapshot_section(
                            section_bytes,
                            progression_snapshot_records,
                        );
                    },
                )
            }
            WorldSnapshotSectionKind::SimulationTime => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    1,
                    |section_bytes| {
                        simulation_time_snapshot_record
                            .append_encoded_simulation_time_snapshot_record(section_bytes);
                    },
                )
            }
            WorldSnapshotSectionKind::Shows => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                show_stage_snapshot_records
                    .len()
                    .saturating_add(scheduled_show_snapshot_records.len())
                    .saturating_add(scheduled_show_break_snapshot_records.len())
                    as u32,
                |section_bytes| {
                    append_encoded_show_snapshot_section(
                        section_bytes,
                        show_stage_snapshot_records,
                        scheduled_show_snapshot_records,
                        scheduled_show_break_snapshot_records,
                    );
                },
            ),
            WorldSnapshotSectionKind::Transport => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    transport_snapshot_records
                        .circuit_records
                        .len()
                        .saturating_add(transport_snapshot_records.circuit_member_records.len())
                        .saturating_add(transport_snapshot_records.vehicle_records.len())
                        as u32,
                    |section_bytes| {
                        append_encoded_transport_snapshot_section(
                            section_bytes,
                            transport_snapshot_records,
                        );
                    },
                )
            }
            WorldSnapshotSectionKind::Photos => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                photo_snapshot_records
                    .len()
                    .saturating_add(photo_challenge_progress_snapshot_records.len())
                    as u32,
                |section_bytes| {
                    append_encoded_photo_snapshot_section(
                        section_bytes,
                        photo_snapshot_records,
                        photo_challenge_progress_snapshot_records,
                    );
                },
            ),
            _ => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                0,
                |_| {},
            ),
        })
        .and_then(|()| {
            finalize_world_snapshot_container_header_and_checksum(
                snapshot_container_bytes,
                tank_snapshot_records
                    .len()
                    .saturating_add(topology_snapshot_records.len())
                    .saturating_add(animal_snapshot_records.len())
                    .saturating_add(
                        super::scenario_snapshot_section::persistent_ids(scenario_snapshot).count(),
                    )
                    .saturating_add(donation_snapshot_records.len())
                    .saturating_add(placement_snapshot_records.len())
                    .saturating_add(maintenance_snapshot.waste.len())
                    .saturating_add(staff_snapshot_records.len())
                    .saturating_add(show_stage_snapshot_records.len())
                    .saturating_add(scheduled_show_snapshot_records.len())
                    .saturating_add(scheduled_show_break_snapshot_records.len())
                    .saturating_add(transport_snapshot_records.circuit_records.len())
                    .saturating_add(transport_snapshot_records.circuit_member_records.len())
                    .saturating_add(transport_snapshot_records.vehicle_records.len())
                    .saturating_add(1)
                    .saturating_add(economy_snapshot_records.monthly_finance_records.len())
                    .saturating_add(economy_snapshot_records.facility_economy_records.len())
                    .saturating_add(photo_snapshot_records.len())
                    .saturating_add(photo_challenge_progress_snapshot_records.len())
                    .saturating_add(progression_snapshot_records.snapshot_record_count() as usize)
                    .saturating_add(1) as u32,
            )
            .map(|_| ())
        })
}

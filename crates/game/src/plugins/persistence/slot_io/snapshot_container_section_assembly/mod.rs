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
    animal_adoption_offer_inventory_snapshot_section::append_encoded_animal_adoption_offer_inventory_snapshot_section,
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
    animal_adoption_offer_inventory: &crate::plugins::animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory,
    tank_snapshot_records: &[TankSnapshotRecord],
    economy_snapshot_records: &EconomySnapshotRecords,
    progression_snapshot_records: &ProgressionSnapshotRecords,
    simulation_time_snapshot_record: &SimulationTimeSnapshotRecord,
    show_stage_snapshot_records: &[ShowStageSnapshotRecord],
    scheduled_show_snapshot_records: &[ScheduledShowSnapshotRecord],
    scheduled_show_break_snapshot_records: &[ScheduledShowBreakSnapshotRecord],
    transport_snapshot_records: &TransportSnapshotRecords,
    photo_snapshot_records: &[PhotoSnapshotRecord],
    photo_challenge_progress_snapshot_records: &[PhotoChallengeProgressSnapshotRecord],
) -> Result<(), WorldSnapshotPersistenceFailure> {
    WorldSnapshotSectionKind::ENCODING_ORDER
        .into_iter()
        .try_for_each(|snapshot_section| match snapshot_section {
            WorldSnapshotSectionKind::World => append_encoded_world_snapshot_section_to_container(
                snapshot_container_bytes,
                snapshot_section,
                1,
                |section_bytes| {
                    append_encoded_world_snapshot_section(section_bytes, world_snapshot_record);
                },
            ),
            WorldSnapshotSectionKind::Animals => {
                append_encoded_world_snapshot_section_to_container(
                    snapshot_container_bytes,
                    snapshot_section,
                    animal_adoption_offer_inventory.slot_count() as u32,
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
                    0,
                    |_| {},
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
                    .saturating_add(animal_adoption_offer_inventory.slot_count())
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

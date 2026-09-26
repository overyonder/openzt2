use openzt2_game_data::AssetId;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::show_snapshot_types::{
    ScheduledShowBreakSnapshotRecord, ScheduledShowPerformanceSnapshotRecord,
    ScheduledShowSnapshotRecord, ShowSnapshotRecords, ShowStageSnapshotRecord,
};

pub(super) fn append_encoded_show_snapshot_section(
    bytes: &mut Vec<u8>,
    stages: &[ShowStageSnapshotRecord],
    scheduled: &[ScheduledShowSnapshotRecord],
    breaks: &[ScheduledShowBreakSnapshotRecord],
) {
    bytes.extend_from_slice(&(stages.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(scheduled.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(breaks.len() as u32).to_le_bytes());
    for show in stages {
        bytes.extend_from_slice(&show.persistent_identifier.0.to_le_bytes());
        bytes.push(u8::from(show.is_open));
        bytes.extend_from_slice(
            &show
                .canopy_upgrade_persistent_identifier
                .map_or(0, |id| id.0)
                .to_le_bytes(),
        );
        bytes.extend_from_slice(
            &show
                .television_upgrade_persistent_identifier
                .map_or(0, |id| id.0)
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&(show.display_name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(show.display_name.as_bytes());
    }
    for show in scheduled {
        bytes.extend_from_slice(&show.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&show.stage_persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&show.schedule_row_order.to_le_bytes());
        bytes.push(u8::from(show.is_active));
        bytes.extend_from_slice(&show.performance_capacity.to_le_bytes());
        bytes.extend_from_slice(&(show.performance_records.len() as u16).to_le_bytes());
        for slot in &show.performance_records {
            bytes.extend_from_slice(&slot.performer_persistent_identifier.0.to_le_bytes());
            bytes.extend_from_slice(&slot.trick_definition_identifier.0);
            bytes.extend_from_slice(&slot.duration_simulation_ticks.to_le_bytes());
        }
    }
    for pause in breaks {
        bytes.extend_from_slice(&pause.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&pause.stage_persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&pause.schedule_row_order.to_le_bytes());
        bytes.push(u8::from(pause.is_active));
        bytes.extend_from_slice(&pause.duration_simulation_ticks.to_le_bytes());
    }
}

pub(super) fn decode_and_validate_show_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<ShowSnapshotRecords, WorldSnapshotPersistenceFailure> {
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    let counts = payload
        .get(0..12)
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let stage_count = u32::from_le_bytes(
        counts[0..4]
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    ) as usize;
    let scheduled_count = u32::from_le_bytes(
        counts[4..8]
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    ) as usize;
    let break_count = u32::from_le_bytes(
        counts[8..12]
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
    ) as usize;
    if stage_count
        .saturating_add(scheduled_count)
        .saturating_add(break_count)
        != range.encoded_record_count as usize
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut cursor = 12usize;
    let mut stages = Vec::with_capacity(stage_count);
    for _ in 0..stage_count {
        let header = payload
            .get(cursor..cursor.saturating_add(29))
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let id =
            PersistentId(u64::from_le_bytes(header[0..8].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let enabled = match header[8] {
            0 => false,
            1 => true,
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        };
        let canopy =
            PersistentId(u64::from_le_bytes(header[9..17].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let television =
            PersistentId(u64::from_le_bytes(header[17..25].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let name_len = u32::from_le_bytes(
            header[25..29]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        ) as usize;
        cursor += 29;
        let name_bytes = payload
            .get(cursor..cursor.saturating_add(name_len))
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .to_owned();
        cursor += name_len;
        if id.0 == 0
            || stages
                .last()
                .is_some_and(|prior: &ShowStageSnapshotRecord| {
                    prior.persistent_identifier.0 >= id.0
                })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        stages.push(ShowStageSnapshotRecord {
            persistent_identifier: id,
            is_open: enabled,
            display_name: name,
            canopy_upgrade_persistent_identifier: (canopy.0 != 0).then_some(canopy),
            television_upgrade_persistent_identifier: (television.0 != 0).then_some(television),
        });
    }
    let mut scheduled = Vec::with_capacity(scheduled_count);
    for _ in 0..scheduled_count {
        let header = payload
            .get(cursor..cursor.saturating_add(23))
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let id =
            PersistentId(u64::from_le_bytes(header[0..8].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let stage =
            PersistentId(u64::from_le_bytes(header[8..16].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let order = u16::from_le_bytes(
            header[16..18]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        let enabled = match header[18] {
            0 => false,
            1 => true,
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        };
        let capacity = u16::from_le_bytes(
            header[19..21]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        let slot_count = u16::from_le_bytes(
            header[21..23]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        ) as usize;
        cursor += 23;
        if slot_count > usize::from(capacity) {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        let mut slots = Vec::with_capacity(capacity.into());
        for _ in 0..slot_count {
            let slot = payload
                .get(cursor..cursor.saturating_add(28))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let performer =
                PersistentId(u64::from_le_bytes(slot[0..8].try_into().map_err(|_| {
                    WorldSnapshotPersistenceFailure::CorruptSnapshotSection
                })?));
            let mut trick = [0; 16];
            trick.copy_from_slice(&slot[8..24]);
            let duration_ticks = u32::from_le_bytes(
                slot[24..28]
                    .try_into()
                    .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
            );
            if performer.0 == 0 || duration_ticks == 0 {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            slots.push(ScheduledShowPerformanceSnapshotRecord {
                performer_persistent_identifier: performer,
                trick_definition_identifier: AssetId(trick),
                duration_simulation_ticks: duration_ticks,
            });
            cursor += 28;
        }
        if id.0 == 0
            || stage.0 == 0
            || stages.iter().any(|saved| saved.persistent_identifier == id)
            || !stages
                .iter()
                .any(|saved| saved.persistent_identifier == stage)
            || scheduled
                .last()
                .is_some_and(|prior: &ScheduledShowSnapshotRecord| {
                    prior.persistent_identifier.0 >= id.0
                })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        scheduled.push(ScheduledShowSnapshotRecord {
            persistent_identifier: id,
            stage_persistent_identifier: stage,
            schedule_row_order: order,
            is_active: enabled,
            performance_capacity: capacity,
            performance_records: slots,
        });
    }
    let mut breaks = Vec::with_capacity(break_count);
    for _ in 0..break_count {
        let record = payload
            .get(cursor..cursor.saturating_add(23))
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let id =
            PersistentId(u64::from_le_bytes(record[0..8].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let stage =
            PersistentId(u64::from_le_bytes(record[8..16].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let order = u16::from_le_bytes(
            record[16..18]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        let enabled = match record[18] {
            0 => false,
            1 => true,
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        };
        let duration_ticks = u32::from_le_bytes(
            record[19..23]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        cursor += 23;
        if id.0 == 0
            || stage.0 == 0
            || duration_ticks == 0
            || stages.iter().any(|saved| saved.persistent_identifier == id)
            || scheduled
                .iter()
                .any(|saved| saved.persistent_identifier == id)
            || !stages
                .iter()
                .any(|saved| saved.persistent_identifier == stage)
            || breaks
                .last()
                .is_some_and(|prior: &ScheduledShowBreakSnapshotRecord| {
                    prior.persistent_identifier.0 >= id.0
                })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        breaks.push(ScheduledShowBreakSnapshotRecord {
            persistent_identifier: id,
            stage_persistent_identifier: stage,
            schedule_row_order: order,
            is_active: enabled,
            duration_simulation_ticks: duration_ticks,
        });
    }
    (cursor == payload.len())
        .then_some(ShowSnapshotRecords {
            stage_records: stages,
            scheduled_show_records: scheduled,
            scheduled_break_records: breaks,
        })
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}

use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::show_snapshot_types::{
    RunningShowContinuation, RunningShowSnapshot, ScheduledShowBreakSnapshotRecord,
    ScheduledShowPerformanceSnapshotRecord, ScheduledShowSnapshotRecord, ShowSnapshotRecords,
    ShowStageSnapshotRecord,
};

const SHOW_RATINGS_EXTENSION: &[u8; 4] = b"SRAT";
const RUNNING_SHOW_EXTENSION: &[u8; 4] = b"SRUN";
const SHOW_CONTINUATION_EXTENSION: &[u8; 4] = b"SCN1";
const ABSENT_SHOW_RATING: u32 = u32::MAX;

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
        let cells = show
            .performance_records
            .iter()
            .any(|slot| slot.time_slot.is_some());
        bytes.push(u8::from(show.is_active) | (u8::from(cells) << 1));
        bytes.extend_from_slice(&show.performance_capacity.to_le_bytes());
        bytes.extend_from_slice(&(show.performance_records.len() as u16).to_le_bytes());
        for slot in &show.performance_records {
            bytes.extend_from_slice(&slot.performer_persistent_identifier.0.to_le_bytes());
            bytes.extend_from_slice(&slot.trick_definition_identifier.unwrap_or_default().0);
            bytes.extend_from_slice(&slot.duration_simulation_ticks.to_le_bytes());
            if cells {
                bytes.push(u8::from(slot.time_slot.is_some()));
                bytes.extend_from_slice(&slot.time_slot.unwrap_or_default().to_le_bytes());
            }
        }
    }
    for pause in breaks {
        bytes.extend_from_slice(&pause.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&pause.stage_persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&pause.schedule_row_order.to_le_bytes());
        bytes.push(u8::from(pause.is_active));
        bytes.extend_from_slice(&pause.duration_simulation_ticks.to_le_bytes());
    }
    if scheduled.iter().any(|show| show.rating.is_some()) {
        bytes.extend_from_slice(SHOW_RATINGS_EXTENSION);
        for show in scheduled {
            bytes.extend_from_slice(
                &show
                    .rating
                    .map_or(ABSENT_SHOW_RATING, f32::to_bits)
                    .to_le_bytes(),
            );
        }
    }
    let running_count = stages
        .iter()
        .filter(|stage| stage.running.is_some())
        .count() as u32;
    if running_count != 0 {
        bytes.extend_from_slice(RUNNING_SHOW_EXTENSION);
        bytes.extend_from_slice(&running_count.to_le_bytes());
        for stage in stages {
            let Some(running) = stage.running.as_ref() else {
                continue;
            };
            bytes.extend_from_slice(&stage.persistent_identifier.0.to_le_bytes());
            bytes.extend_from_slice(&running.row_id.to_le_bytes());
            bytes.push(running.phase);
            bytes.extend_from_slice(&running.current_slot.to_le_bytes());
            bytes.extend_from_slice(&running.trick_score.to_le_bytes());
            bytes.extend_from_slice(&running.stars.to_le_bytes());
            bytes.extend_from_slice(&running.completed_tricks.to_le_bytes());
            bytes.extend_from_slice(&running.failed_tricks.to_le_bytes());
            bytes.extend_from_slice(&running.remaining_ns.to_le_bytes());
        }
    }
    let continuations = stages
        .iter()
        .filter_map(|stage| {
            stage
                .running
                .as_ref()?
                .continuation
                .as_ref()
                .map(|saved| (stage.persistent_identifier.0, saved))
        })
        .collect::<Vec<_>>();
    if !continuations.is_empty() {
        bytes.extend_from_slice(SHOW_CONTINUATION_EXTENSION);
        bytes.extend(
            postcard::to_allocvec(&continuations)
                .expect("show continuation uses serializable fields"),
        );
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
            running: None,
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
        if header[18] & !3 != 0 {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        let enabled = header[18] & 1 != 0;
        let cells = header[18] & 2 != 0;
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
        let mut slots = Vec::with_capacity(slot_count);
        let mut occupied_cells = std::collections::BTreeSet::new();
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
            cursor += 28;
            let time_slot = if cells {
                let cell = payload
                    .get(cursor..cursor.saturating_add(3))
                    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
                cursor += 3;
                let index = u16::from_le_bytes([cell[1], cell[2]]);
                match cell[0] {
                    0 if index == 0 => None,
                    1 if occupied_cells.insert((performer.0, index)) => Some(index),
                    _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
                }
            } else {
                None
            };
            let trick = (AssetId(trick) != AssetId::default()).then_some(AssetId(trick));
            if performer.0 == 0 || duration_ticks == 0 || (trick.is_none() && time_slot.is_none()) {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            slots.push(ScheduledShowPerformanceSnapshotRecord {
                performer_persistent_identifier: performer,
                trick_definition_identifier: trick,
                time_slot,
                duration_simulation_ticks: duration_ticks,
            });
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
            rating: None,
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
    use WorldSnapshotPersistenceFailure::CorruptSnapshotSection;
    let mut remaining = &payload[cursor..];
    if let Some(data) = remaining.strip_prefix(SHOW_RATINGS_EXTENSION) {
        let (ratings, next) = data
            .split_at_checked(scheduled.len().saturating_mul(size_of::<u32>()))
            .ok_or(CorruptSnapshotSection)?;
        remaining = next;
        for (row, bits) in scheduled.iter_mut().zip(ratings.as_chunks::<4>().0) {
            let bits = u32::from_le_bytes(*bits);
            if bits != ABSENT_SHOW_RATING {
                let rating = f32::from_bits(bits);
                if !rating.is_finite() {
                    return Err(CorruptSnapshotSection);
                }
                row.rating = Some(rating);
            }
        }
    }
    if let Some(data) = remaining.strip_prefix(RUNNING_SHOW_EXTENSION) {
        let (header, data) = data.split_at_checked(4).ok_or(CorruptSnapshotSection)?;
        let count =
            u32::from_le_bytes(header.try_into().map_err(|_| CorruptSnapshotSection)?) as usize;
        const RUNNING_SHOW_BYTES: usize = 45;
        if count == 0
            || count > stages.len()
            || data.len() < count.saturating_mul(RUNNING_SHOW_BYTES)
        {
            return Err(CorruptSnapshotSection);
        }
        let (rows, next) = data.split_at(count * RUNNING_SHOW_BYTES);
        for row in rows.chunks_exact(RUNNING_SHOW_BYTES) {
            let u64_at = |offset| u64::from_le_bytes(row[offset..offset + 8].try_into().unwrap());
            let u32_at = |offset| u32::from_le_bytes(row[offset..offset + 4].try_into().unwrap());
            let stage_id = u64_at(0);
            let running = RunningShowSnapshot {
                row_id: u64_at(8),
                phase: row[16],
                current_slot: u32_at(17),
                trick_score: f32::from_bits(u32_at(21)),
                stars: f32::from_bits(u32_at(25)),
                completed_tricks: u32_at(29),
                failed_tricks: u32_at(33),
                remaining_ns: u64_at(37),
                continuation: None,
            };
            let stage = stages
                .binary_search_by_key(&stage_id, |stage| stage.persistent_identifier.0)
                .ok()
                .map(|index| &mut stages[index])
                .ok_or(CorruptSnapshotSection)?;
            let matches_row = |id: PersistentId, owner: PersistentId| {
                id.0 == running.row_id && owner.0 == stage_id
            };
            if stage.running.is_some()
                || running.phase().is_none()
                || !running.stars.is_finite()
                || !running.trick_score.is_finite()
                || !(scheduled.iter().any(|row| {
                    matches_row(row.persistent_identifier, row.stage_persistent_identifier)
                }) || breaks.iter().any(|row| {
                    matches_row(row.persistent_identifier, row.stage_persistent_identifier)
                }))
            {
                return Err(CorruptSnapshotSection);
            }
            stage.running = Some(running);
        }
        remaining = next;
    }
    if let Some(data) = remaining.strip_prefix(SHOW_CONTINUATION_EXTENSION) {
        let (restored, rest) =
            postcard::take_from_bytes::<Vec<(u64, RunningShowContinuation)>>(data)
                .map_err(|_| CorruptSnapshotSection)?;
        if restored.is_empty() || restored.len() > stages.len() || !rest.is_empty() {
            return Err(CorruptSnapshotSection);
        }
        for (stage_id, continuation) in restored {
            let index = stages
                .binary_search_by_key(&stage_id, |stage| stage.persistent_identifier.0)
                .map_err(|_| CorruptSnapshotSection)?;
            let running = stages[index]
                .running
                .as_mut()
                .ok_or(CorruptSnapshotSection)?;
            let plan = scheduled
                .iter()
                .find(|row| row.persistent_identifier.0 == running.row_id);
            let slots = plan.map_or(&[][..], |plan| plan.performance_records.as_slice());
            let valid = |slot: u32| (slot as usize) < slots.len();
            if running.continuation.is_some()
                || running.current_slot as usize > continuation.columns.len()
                || continuation
                    .columns
                    .iter()
                    .any(|column| column.is_empty() || !column.iter().copied().all(valid))
                || continuation
                    .scores
                    .as_ref()
                    .is_some_and(|(predicted, actual)| {
                        predicted
                            .iter()
                            .chain(actual)
                            .any(|(slot, score)| !valid(*slot) || !score.is_finite())
                            || !predicted.windows(2).all(|pair| pair[0].0 < pair[1].0)
                            || !actual.windows(2).all(|pair| pair[0].0 < pair[1].0)
                    })
                || !continuation
                    .tricks
                    .windows(2)
                    .all(|pair| pair[0].slot < pair[1].slot)
                || continuation.tricks.iter().any(|trick| {
                    let Some(slot) = slots.get(trick.slot as usize) else {
                        return true;
                    };
                    slot.performer_persistent_identifier.0 != trick.performer
                        || slot.trick_definition_identifier != Some(trick.definition)
                        || (trick.pending_token_duration.is_some() && !trick.tokens.is_empty())
                        || trick.tokens.iter().any(|token| {
                            token.available_at > token.expires_at
                                || token.execution == Some(0)
                                || token
                                    .destination
                                    .is_some_and(|value| !value.into_iter().all(f32::is_finite))
                        })
                })
            {
                return Err(CorruptSnapshotSection);
            }
            running.continuation = Some(continuation);
        }
        remaining = rest;
    }
    if !remaining.is_empty() {
        return Err(CorruptSnapshotSection);
    }
    Ok(ShowSnapshotRecords {
        stage_records: stages,
        scheduled_show_records: scheduled,
        scheduled_break_records: breaks,
    })
}

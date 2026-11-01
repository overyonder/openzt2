use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalState;
use openzt2_zoo_state::guests::guest_survey_types::GuestSurvey;

pub(super) fn append_runtime_continuation_snapshot(
    bytes: &mut Vec<u8>,
    record: &RuntimeContinuationSnapshot,
) -> Result<(), Failure> {
    validate_runtime_continuation_snapshot(record)?;
    bytes.push(3);
    bytes.extend_from_slice(
        &(u16::from(record.service_request_sequence.is_some())
            | (u16::from(record.guest_arrivals.is_some()) << 1)
            | (u16::from(record.guest_survey.is_some()) << 2)
            | (u16::from(record.guest_favourite_animal_trend.is_some()) << 3)
            | (u16::from(record.entity_name_counters.is_some()) << 4)
            | (u16::from(record.registered_entity_name_identifiers.is_some()) << 5)
            | (u16::from(record.world_audio_preset.is_some()) << 6)
            | (u16::from(record.entity_names.is_some()) << 7)
            | (u16::from(record.general_economy.is_some()) << 8))
            .to_le_bytes(),
    );
    for value in [
        record.next_persistent_identifier,
        record.navigation_sequence,
        record.behavior_execution_sequence,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    if let Some(sequence) = record.service_request_sequence {
        bytes.extend_from_slice(&sequence.to_le_bytes());
    }
    if let Some(arrivals) = record.guest_arrivals {
        for value in [
            arrivals.next_tick,
            arrivals.rng.to_raw()[0],
            arrivals.rng.to_raw()[1],
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Some(survey) = record.guest_survey {
        bytes.extend_from_slice(&survey.critical_need_countdown_seconds.to_le_bytes());
        bytes.extend_from_slice(&survey.previous_day_of_month.to_le_bytes());
        bytes.extend_from_slice(&survey.current_day_of_month.to_le_bytes());
        for history in survey.ratings {
            bytes.extend_from_slice(&history.cumulative_rating.to_le_bytes());
            bytes.extend_from_slice(&history.cumulative_count.to_le_bytes());
            for value in history.daily_ratings {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            for value in history.daily_counts {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    if let Some(trend) = record.guest_favourite_animal_trend {
        let species = trend.species();
        bytes.push(u8::from(species[0].is_some()) | (u8::from(species[1].is_some()) << 1));
        bytes.push(trend.spawn_threshold_index());
        bytes.extend_from_slice(&trend.spawn_count().to_le_bytes());
        for species in species.into_iter().flatten() {
            bytes.extend_from_slice(&species.0);
        }
    }
    if let Some(counters) = &record.entity_name_counters {
        let entries = counters.entries().collect::<Vec<_>>();
        let count = u32::try_from(entries.len()).map_err(|_| Failure::CapacityExceeded)?;
        bytes.extend_from_slice(&count.to_le_bytes());
        for (tag, count) in entries {
            bytes.extend_from_slice(&tag.0);
            bytes.extend_from_slice(&count.to_le_bytes());
        }
    }
    if let Some(ids) = &record.registered_entity_name_identifiers {
        bytes.extend_from_slice(
            &u32::try_from(ids.len())
                .map_err(|_| Failure::CapacityExceeded)?
                .to_le_bytes(),
        );
        for id in ids {
            bytes.extend_from_slice(&id.to_le_bytes());
        }
    }
    if let Some(preset) = &record.world_audio_preset {
        let payload = postcard::to_allocvec(preset).map_err(|_| Failure::CorruptSnapshotSection)?;
        bytes.extend_from_slice(
            &u32::try_from(payload.len())
                .map_err(|_| Failure::CapacityExceeded)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&payload);
    }
    if let Some(names) = &record.entity_names {
        let payload = postcard::to_allocvec(names).map_err(|_| Failure::CorruptSnapshotSection)?;
        bytes.extend_from_slice(
            &u32::try_from(payload.len())
                .map_err(|_| Failure::CapacityExceeded)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&payload);
    }
    if let Some(general) = &record.general_economy {
        let payload = super::monthly_finance_history_encoding::encode_monthly_finance_history(
            general.history(),
        );
        bytes.extend_from_slice(
            &u32::try_from(payload.len())
                .map_err(|_| Failure::CapacityExceeded)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&payload);
    }
    Ok(())
}

pub(super) fn decode_runtime_continuation_snapshot(
    mut bytes: &[u8],
) -> Result<RuntimeContinuationSnapshot, Failure> {
    let revision = take::<1>(&mut bytes)?[0];
    if revision != 3 {
        return Err(Failure::CorruptSnapshotSection);
    }
    let flags = u16::from_le_bytes(take(&mut bytes)?);
    if flags & !511 != 0 {
        return Err(Failure::CorruptSnapshotSection);
    }
    let next_persistent_identifier = u64::from_le_bytes(take(&mut bytes)?);
    let navigation_sequence = u64::from_le_bytes(take(&mut bytes)?);
    let behavior_execution_sequence = u64::from_le_bytes(take(&mut bytes)?);
    let service_request_sequence = if flags & 1 != 0 {
        Some(u64::from_le_bytes(take(&mut bytes)?))
    } else {
        None
    };
    let guest_arrivals = if flags & 2 != 0 {
        Some(GuestArrivalState {
            next_tick: u64::from_le_bytes(take(&mut bytes)?),
            rng: DeterministicRng::from_raw([
                u64::from_le_bytes(take(&mut bytes)?),
                u64::from_le_bytes(take(&mut bytes)?),
            ]),
        })
    } else {
        None
    };
    let guest_survey = if flags & 4 != 0 {
        let mut survey = GuestSurvey {
            critical_need_countdown_seconds: f32::from_le_bytes(take(&mut bytes)?),
            previous_day_of_month: i32::from_le_bytes(take(&mut bytes)?),
            current_day_of_month: i32::from_le_bytes(take(&mut bytes)?),
            ..Default::default()
        };
        for history in &mut survey.ratings {
            history.cumulative_rating = f32::from_le_bytes(take(&mut bytes)?);
            history.cumulative_count = u32::from_le_bytes(take(&mut bytes)?);
            for value in &mut history.daily_ratings {
                *value = f32::from_le_bytes(take(&mut bytes)?);
            }
            for value in &mut history.daily_counts {
                *value = u32::from_le_bytes(take(&mut bytes)?);
            }
        }
        Some(survey)
    } else {
        None
    };
    let guest_favourite_animal_trend = if flags & 8 != 0 {
        let species_flags = take::<1>(&mut bytes)?[0];
        if species_flags & !3 != 0 {
            return Err(Failure::CorruptSnapshotSection);
        }
        let index = take::<1>(&mut bytes)?[0];
        let count = u32::from_le_bytes(take(&mut bytes)?);
        let mut species = [None; 2];
        for (index, value) in species.iter_mut().enumerate() {
            if species_flags & (1 << index) != 0 {
                *value = Some(AssetId(take(&mut bytes)?));
            }
        }
        Some(
            GuestFavouriteAnimalTrend::from_saved_state(species, count, index)
                .ok_or(Failure::CorruptSnapshotSection)?,
        )
    } else {
        None
    };
    let entity_name_counters = if flags & 16 != 0 {
        let count = u32::from_le_bytes(take(&mut bytes)?) as usize;
        if count > bytes.len() / 20 {
            return Err(Failure::CorruptSnapshotSection);
        }
        let entries = (0..count)
            .map(|_| {
                Ok((
                    AssetId(take(&mut bytes)?),
                    u32::from_le_bytes(take(&mut bytes)?),
                ))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        Some(
            openzt2_zoo_state::entity_naming_types::EntityNameTagCounters::from_saved_entries(
                entries,
            )
            .ok_or(Failure::CorruptSnapshotSection)?,
        )
    } else {
        None
    };
    let registered_entity_name_identifiers = if flags & 32 != 0 {
        let count = u32::from_le_bytes(take(&mut bytes)?) as usize;
        if count > bytes.len() / 8 {
            return Err(Failure::CorruptSnapshotSection);
        }
        Some(
            (0..count)
                .map(|_| Ok(u64::from_le_bytes(take(&mut bytes)?)))
                .collect::<Result<Vec<_>, Failure>>()?,
        )
    } else {
        None
    };
    let world_audio_preset = if flags & 64 != 0 {
        let length = u32::from_le_bytes(take(&mut bytes)?) as usize;
        let (payload, rest) = bytes
            .split_at_checked(length)
            .ok_or(Failure::CorruptSnapshotSection)?;
        bytes = rest;
        let (document, trailing) =
            postcard::take_from_bytes(payload).map_err(|_| Failure::CorruptSnapshotSection)?;
        if !trailing.is_empty() {
            return Err(Failure::CorruptSnapshotSection);
        }
        Some(document)
    } else {
        None
    };
    let entity_names = if flags & 128 != 0 {
        let length = u32::from_le_bytes(take(&mut bytes)?) as usize;
        let (payload, rest) = bytes
            .split_at_checked(length)
            .ok_or(Failure::CorruptSnapshotSection)?;
        bytes = rest;
        let (names, trailing) =
            postcard::take_from_bytes(payload).map_err(|_| Failure::CorruptSnapshotSection)?;
        if !trailing.is_empty() {
            return Err(Failure::CorruptSnapshotSection);
        }
        Some(names)
    } else {
        None
    };
    let general_economy = if flags & 256 != 0 {
        let length = u32::from_le_bytes(take(&mut bytes)?) as usize;
        let (payload, rest) = bytes
            .split_at_checked(length)
            .ok_or(Failure::CorruptSnapshotSection)?;
        bytes = rest;
        Some(
            openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger::from_saved(
                super::monthly_finance_history_encoding::decode_monthly_finance_history(payload)?,
            )
            .ok_or(Failure::CorruptSnapshotSection)?,
        )
    } else {
        None
    };
    if !bytes.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    let record = RuntimeContinuationSnapshot {
        general_economy,
        entity_names,
        world_audio_preset,
        entity_name_counters,
        registered_entity_name_identifiers,
        next_persistent_identifier,
        navigation_sequence,
        behavior_execution_sequence,
        service_request_sequence,
        guest_arrivals,
        guest_survey,
        guest_favourite_animal_trend,
    };
    validate_runtime_continuation_snapshot(&record)?;
    Ok(record)
}

pub(super) fn validate_runtime_continuation_snapshot(
    record: &RuntimeContinuationSnapshot,
) -> Result<(), Failure> {
    if record.entity_names.as_ref().is_some_and(|names| {
        names.iter().any(|(id, _)| matches!(*id, 0 | u64::MAX))
            || names.windows(2).any(|pair| pair[0].0 >= pair[1].0)
    }) || record
        .world_audio_preset
        .as_ref()
        .is_some_and(|preset| !valid_world_audio_preset(preset))
        || record
            .registered_entity_name_identifiers
            .as_ref()
            .is_some_and(|ids| {
                ids.iter().any(|id| matches!(*id, 0 | u64::MAX))
                    || ids.windows(2).any(|pair| pair[0] >= pair[1])
            })
        || record.next_persistent_identifier == 0
        || record.guest_survey.is_some_and(|survey| {
            !survey.critical_need_countdown_seconds.is_finite()
                || survey.ratings.iter().any(|history| {
                    !history.cumulative_rating.is_finite()
                        || history.daily_ratings.iter().any(|value| !value.is_finite())
                })
        })
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    Ok(())
}

fn valid_world_audio_preset(preset: &openzt2_game_data::audio::AudioDocument) -> bool {
    preset.mixer.is_none()
        && preset.ambient.is_none()
        && preset.water.is_none()
        && preset.cues.is_empty()
        && preset.stages.is_empty()
        && preset.soundscape_biomes.is_empty()
        && preset.soundscape_allowed_entities.is_empty()
        && preset.soundscape_contents.len() == 1
        && preset.soundscape_contents[0]
            .cues
            .iter()
            .all(|cue| cue.weight.is_finite() && cue.weight >= 0.0)
}

fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], Failure> {
    let (prefix, remainder) = bytes
        .split_at_checked(N)
        .ok_or(Failure::CorruptSnapshotSection)?;
    let value = prefix
        .try_into()
        .map_err(|_| Failure::CorruptSnapshotSection)?;
    *bytes = remainder;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_preserves_exhausted_counters_presence_and_next_random_draw() {
        for flags in 0..16 {
            let mut record = RuntimeContinuationSnapshot {
                general_economy: None,
                entity_names: None,
                world_audio_preset: None,
                entity_name_counters: None,
                registered_entity_name_identifiers: None,
                next_persistent_identifier: u64::MAX,
                navigation_sequence: 991,
                behavior_execution_sequence: 845,
                service_request_sequence: (flags & 1 != 0).then_some(u64::MAX),
                guest_arrivals: (flags & 2 != 0).then_some(GuestArrivalState {
                    next_tick: 1234,
                    rng: DeterministicRng::from_raw([123456, 789]),
                }),
                guest_survey: (flags & 4 != 0).then_some(GuestSurvey {
                    critical_need_countdown_seconds: -0.125,
                    previous_day_of_month: 9,
                    current_day_of_month: 1,
                    ..Default::default()
                }),
                guest_favourite_animal_trend: (flags & 8 != 0).then_some(
                    GuestFavouriteAnimalTrend::from_saved_state(
                        [
                            Some(AssetId::from_key("first")),
                            Some(AssetId::from_key("second")),
                        ],
                        37,
                        1,
                    )
                    .unwrap(),
                ),
            };
            if let Some(survey) = &mut record.guest_survey {
                for (index, history) in survey.ratings.iter_mut().enumerate() {
                    history.cumulative_rating = index as f32 + 0.25;
                    history.cumulative_count = index as u32 + 97;
                    for day in 0..31 {
                        history.daily_ratings[day] = (index * 31 + day) as f32 * 0.125;
                        history.daily_counts[day] = (day * 3 + index) as u32;
                    }
                }
            }
            let mut encoded = Vec::new();
            append_runtime_continuation_snapshot(&mut encoded, &record).unwrap();
            let decoded = decode_runtime_continuation_snapshot(&encoded).unwrap();
            assert_eq!(decoded.next_persistent_identifier, u64::MAX);
            assert_eq!(decoded.navigation_sequence, 991);
            assert_eq!(decoded.behavior_execution_sequence, 845);
            assert_eq!(
                decoded.service_request_sequence,
                record.service_request_sequence
            );
            assert_eq!(decoded.guest_survey, record.guest_survey);
            assert_eq!(
                decoded.guest_favourite_animal_trend,
                record.guest_favourite_animal_trend
            );
            assert_eq!(
                decoded
                    .guest_arrivals
                    .map(|mut arrivals| (arrivals.next_tick, arrivals.rng.next_u32())),
                record
                    .guest_arrivals
                    .map(|mut arrivals| (arrivals.next_tick, arrivals.rng.next_u32()))
            );
            let mut encoded_again = Vec::new();
            append_runtime_continuation_snapshot(&mut encoded_again, &decoded).unwrap();
            assert_eq!(encoded_again, encoded);
            encoded.push(0);
            assert!(decode_runtime_continuation_snapshot(&encoded).is_err());
        }
    }

    #[test]
    fn naming_absence_and_invalid_registration_lists_are_distinct() {
        let mut record = RuntimeContinuationSnapshot {
            general_economy: None,
            entity_names: None,
            world_audio_preset: None,
            entity_name_counters: None,
            registered_entity_name_identifiers: None,
            next_persistent_identifier: 10,
            navigation_sequence: 0,
            behavior_execution_sequence: 0,
            service_request_sequence: None,
            guest_arrivals: None,
            guest_survey: None,
            guest_favourite_animal_trend: None,
        };
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &record).unwrap();
        let absent = decode_runtime_continuation_snapshot(&bytes).unwrap();
        assert!(absent.registered_entity_name_identifiers.is_none());
        for ids in [vec![0], vec![u64::MAX], vec![2, 2], vec![3, 2]] {
            record.registered_entity_name_identifiers = Some(ids);
            assert!(append_runtime_continuation_snapshot(&mut Vec::new(), &record).is_err());
        }
        record.registered_entity_name_identifiers = Some(Vec::new());
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &record).unwrap();
        assert_eq!(
            decode_runtime_continuation_snapshot(&bytes)
                .unwrap()
                .registered_entity_name_identifiers,
            Some(Vec::new())
        );
        bytes[0] = 2;
        assert!(decode_runtime_continuation_snapshot(&bytes).is_err());
    }

    #[test]
    fn malformed_headers_and_truncated_continuations_are_rejected() {
        let record = RuntimeContinuationSnapshot {
            general_economy: None,
            entity_names: None,
            world_audio_preset: None,
            entity_name_counters: None,
            registered_entity_name_identifiers: None,
            next_persistent_identifier: 1,
            navigation_sequence: 0,
            behavior_execution_sequence: 0,
            service_request_sequence: None,
            guest_arrivals: None,
            guest_survey: None,
            guest_favourite_animal_trend: None,
        };
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &record).unwrap();
        for length in 0..bytes.len() {
            assert!(decode_runtime_continuation_snapshot(&bytes[..length]).is_err());
        }
        let mut bad_version = bytes.clone();
        bad_version[0] = 4;
        assert!(decode_runtime_continuation_snapshot(&bad_version).is_err());
        let mut unknown_flags = bytes.clone();
        unknown_flags[2] = 2;
        assert!(decode_runtime_continuation_snapshot(&unknown_flags).is_err());
        bytes[3..11].fill(0);
        assert!(decode_runtime_continuation_snapshot(&bytes).is_err());
    }
}

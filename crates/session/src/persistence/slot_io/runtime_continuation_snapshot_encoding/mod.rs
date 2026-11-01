use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_zoo_state::guests::guest_simulation_types::{GuestArrivalState, GuestSurvey};

pub(super) fn append_runtime_continuation_snapshot(
    bytes: &mut Vec<u8>,
    record: &RuntimeContinuationSnapshot,
) -> Result<(), Failure> {
    validate_runtime_continuation_snapshot(record)?;
    bytes.push(1);
    bytes.push(
        u8::from(record.service_request_sequence.is_some())
            | (u8::from(record.guest_arrivals.is_some()) << 1)
            | (u8::from(record.guest_survey.is_some()) << 2),
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
        bytes.extend_from_slice(&survey.next_tick.to_le_bytes());
        bytes.extend_from_slice(&survey.month.to_le_bytes());
        bytes.extend_from_slice(&survey.critical_hits.to_le_bytes());
        bytes.extend_from_slice(&survey.education_points.to_le_bytes());
        bytes.extend_from_slice(&survey.entertainment_points.to_le_bytes());
        for value in [
            survey.critical_need_rate_permille,
            survey.education_view_rate_permille,
            survey.entertainment_view_rate_permille,
            survey.view_score_permille,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    Ok(())
}

pub(super) fn decode_runtime_continuation_snapshot(
    mut bytes: &[u8],
) -> Result<RuntimeContinuationSnapshot, Failure> {
    if take::<1>(&mut bytes)? != [1] {
        return Err(Failure::CorruptSnapshotSection);
    }
    let flags = take::<1>(&mut bytes)?[0];
    if flags & !7 != 0 {
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
        Some(GuestSurvey {
            next_tick: u64::from_le_bytes(take(&mut bytes)?),
            month: u32::from_le_bytes(take(&mut bytes)?),
            critical_hits: u16::from_le_bytes(take(&mut bytes)?),
            education_points: u32::from_le_bytes(take(&mut bytes)?),
            entertainment_points: u32::from_le_bytes(take(&mut bytes)?),
            critical_need_rate_permille: u16::from_le_bytes(take(&mut bytes)?),
            education_view_rate_permille: u16::from_le_bytes(take(&mut bytes)?),
            entertainment_view_rate_permille: u16::from_le_bytes(take(&mut bytes)?),
            view_score_permille: u16::from_le_bytes(take(&mut bytes)?),
        })
    } else {
        None
    };
    if !bytes.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    let record = RuntimeContinuationSnapshot {
        next_persistent_identifier,
        navigation_sequence,
        behavior_execution_sequence,
        service_request_sequence,
        guest_arrivals,
        guest_survey,
    };
    validate_runtime_continuation_snapshot(&record)?;
    Ok(record)
}

pub(super) fn validate_runtime_continuation_snapshot(
    record: &RuntimeContinuationSnapshot,
) -> Result<(), Failure> {
    if record.next_persistent_identifier == 0
        || record.guest_survey.is_some_and(|survey| {
            [
                survey.critical_need_rate_permille,
                survey.education_view_rate_permille,
                survey.entertainment_view_rate_permille,
                survey.view_score_permille,
            ]
            .into_iter()
            .any(|value| value > 1000)
        })
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    Ok(())
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
        for flags in 0..8 {
            let record = RuntimeContinuationSnapshot {
                next_persistent_identifier: u64::MAX,
                navigation_sequence: 991,
                behavior_execution_sequence: 845,
                service_request_sequence: (flags & 1 != 0).then_some(u64::MAX),
                guest_arrivals: (flags & 2 != 0).then_some(GuestArrivalState {
                    next_tick: 1234,
                    rng: DeterministicRng::from_raw([123456, 789]),
                }),
                guest_survey: (flags & 4 != 0).then_some(GuestSurvey {
                    next_tick: 5432,
                    month: 48,
                    critical_hits: 12,
                    education_points: 20000,
                    entertainment_points: 15000,
                    critical_need_rate_permille: 1000,
                    education_view_rate_permille: 333,
                    entertainment_view_rate_permille: 29,
                    view_score_permille: 117,
                }),
            };
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
    fn malformed_headers_and_truncated_continuations_are_rejected() {
        let record = RuntimeContinuationSnapshot {
            next_persistent_identifier: 1,
            navigation_sequence: 0,
            behavior_execution_sequence: 0,
            service_request_sequence: None,
            guest_arrivals: None,
            guest_survey: None,
        };
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &record).unwrap();
        for length in 0..bytes.len() {
            assert!(decode_runtime_continuation_snapshot(&bytes[..length]).is_err());
        }
        let mut bad_version = bytes.clone();
        bad_version[0] = 2;
        assert!(decode_runtime_continuation_snapshot(&bad_version).is_err());
        let mut unknown_flags = bytes.clone();
        unknown_flags[1] = 8;
        assert!(decode_runtime_continuation_snapshot(&unknown_flags).is_err());
        bytes[2..10].fill(0);
        assert!(decode_runtime_continuation_snapshot(&bytes).is_err());
    }
}

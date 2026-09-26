use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_types::ProgressionSnapshotRecords,
};

pub(super) fn validate_progression_snapshot_record_invariants(
    progression_snapshot_records: &ProgressionSnapshotRecords,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if progression_snapshot_records.fame.half_stars
        > progression_snapshot_records.fame.maximum_reached
        || progression_snapshot_records
            .fame
            .maximum_percent_reached
            .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
        || zoo_rating_component_values(&progression_snapshot_records.zoo_rating)
            .into_iter()
            .any(|rating_permille| rating_permille > 1000)
        || progression_snapshot_records
            .unlocked_catalogue_definition_bit_words
            .len()
            != progression_snapshot_records
                .unlocked_catalogue_definition_count
                .div_ceil(64) as usize
        || progression_snapshot_records
            .fame_history_samples
            .windows(2)
            .any(|adjacent_samples| {
                adjacent_samples[0].month_index >= adjacent_samples[1].month_index
            })
        || progression_snapshot_records
            .fame_history_samples
            .iter()
            .any(|fame_history_sample| {
                fame_history_sample.half_stars > progression_snapshot_records.fame.maximum_reached
            })
        || progression_snapshot_records.research_projects.iter().any(
            |research_project_snapshot_record| {
                research_project_snapshot_record.required_simulation_ticks == 0
                    || research_project_snapshot_record.elapsed_simulation_ticks
                        > research_project_snapshot_record.required_simulation_ticks
            },
        )
        || progression_snapshot_records
            .research_project_availability
            .iter()
            .any(|research_availability_snapshot_record| {
                research_availability_snapshot_record
                    .remaining_unlock_ticks
                    .is_some_and(|remaining_unlock_ticks| {
                        remaining_unlock_ticks == 0
                            || research_availability_snapshot_record.available
                    })
            })
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    validate_unique_non_reserved_progression_snapshot_persistent_identifiers(
        progression_snapshot_records.persistent_identifiers(),
    )
}

fn zoo_rating_component_values(
    zoo_rating: &crate::plugins::progression::rating_types::ZooRating,
) -> [u16; 8] {
    [
        zoo_rating.animal_welfare_permille,
        zoo_rating.guest_satisfaction_permille,
        zoo_rating.education_permille,
        zoo_rating.variety_permille,
        zoo_rating.scenery_permille,
        zoo_rating.finance_permille,
        zoo_rating.cleanliness_permille,
        zoo_rating.overall_permille,
    ]
}

fn validate_unique_non_reserved_progression_snapshot_persistent_identifiers(
    persistent_identifiers: impl Iterator<Item = PersistentId>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut sorted_persistent_identifiers = persistent_identifiers.collect::<Vec<_>>();
    sorted_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);
    if sorted_persistent_identifiers.iter().enumerate().any(
        |(persistent_identifier_index, persistent_identifier)| {
            matches!(persistent_identifier.0, 0 | u64::MAX)
                || persistent_identifier_index > 0
                    && sorted_persistent_identifiers[persistent_identifier_index - 1]
                        == *persistent_identifier
        },
    ) {
        return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
    }
    Ok(())
}

use bevy::prelude::{Entity, Query, With};

use crate::plugins::{
    aquatic::aquatic_simulation_types::Tank,
    economy::facility_economy_types::FacilityProfit,
    photos::{
        photo_album_types::PhotoAlbum,
        photo_challenge_types::{PhotoChallenge, PhotoChallengeProgress},
    },
    shows::show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
    world_spawn::persistent_id_types::PersistentId,
};

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        progression_snapshot_application::ProgressionSnapshotApplicationParameters,
        progression_snapshot_types::ProgressionSnapshotRecords,
    },
    economy_snapshot_types::EconomySnapshotRecords,
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    show_snapshot_types::ShowSnapshotRecords,
    tank_snapshot_types::TankSnapshotRecord,
    transport_snapshot_types::TransportSnapshotRecords,
};

pub(super) fn validate_loaded_snapshot_references_and_imported_persistent_identifiers(
    tank_snapshot_records: &[TankSnapshotRecord],
    economy_snapshot_records: &EconomySnapshotRecords,
    progression_snapshot_records: &ProgressionSnapshotRecords,
    show_snapshot_records: &ShowSnapshotRecords,
    transport_snapshot_records: &TransportSnapshotRecords,
    photo_snapshot_records: &[PhotoSnapshotRecord],
    photo_challenge_progress_snapshot_records: &[PhotoChallengeProgressSnapshotRecord],
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    tank_persistent_identifiers: &Query<&PersistentId, With<Tank>>,
    facility_persistent_identifiers: &Query<&PersistentId, With<FacilityProfit>>,
    show_stages_with_mutable_snapshot_state: &mut Query<
        (
            Entity,
            &PersistentId,
            Option<&mut ShowStageOpenState>,
            Option<&mut ShowName>,
        ),
        With<ShowStage>,
    >,
    photo_album_persistent_identifiers: &Query<&PersistentId, With<PhotoAlbum>>,
    photo_challenges_with_mutable_progress: &mut Query<(
        &PhotoChallenge,
        &mut PhotoChallengeProgress,
    )>,
    progression_snapshot_application_parameters: &ProgressionSnapshotApplicationParameters,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let existing_persistent_identifiers = entities_with_persistent_identifiers
        .iter()
        .map(|(_, persistent_identifier)| *persistent_identifier)
        .collect::<Vec<_>>();
    let persistent_identifier_exists = |persistent_identifier: PersistentId| {
        existing_persistent_identifiers
            .iter()
            .any(|candidate| *candidate == persistent_identifier)
    };

    let every_required_existing_entity_is_present = tank_snapshot_records.iter().all(|record| {
        tank_persistent_identifiers
            .iter()
            .any(|persistent_identifier| *persistent_identifier == record.persistent_identifier)
    }) && economy_snapshot_records
        .facility_economy_records
        .iter()
        .all(|record| {
            facility_persistent_identifiers
                .iter()
                .any(|persistent_identifier| *persistent_identifier == record.persistent_identifier)
        })
        && show_snapshot_records.stage_records.iter().all(|record| {
            show_stages_with_mutable_snapshot_state.iter_mut().any(
                |(_, persistent_identifier, ..)| {
                    *persistent_identifier == record.persistent_identifier
                },
            )
        })
        && photo_snapshot_records.iter().all(|record| {
            photo_album_persistent_identifiers
                .iter()
                .any(|persistent_identifier| {
                    *persistent_identifier == record.album_persistent_identifier
                })
        })
        && show_snapshot_records
            .scheduled_show_records
            .iter()
            .all(|record| {
                persistent_identifier_exists(record.stage_persistent_identifier)
                    && record.performance_records.iter().all(|performance_record| {
                        persistent_identifier_exists(
                            performance_record.performer_persistent_identifier,
                        )
                    })
            })
        && show_snapshot_records
            .scheduled_break_records
            .iter()
            .all(|record| persistent_identifier_exists(record.stage_persistent_identifier))
        && photo_challenge_progress_snapshot_records
            .iter()
            .all(|record| {
                photo_challenges_with_mutable_progress
                    .iter_mut()
                    .any(|(challenge, _)| challenge.row == record.challenge_row_index)
            });

    let photo_persistent_identifiers = photo_snapshot_records
        .iter()
        .map(|record| record.persistent_identifier)
        .collect::<Vec<_>>();
    let every_photo_challenge_completion_refers_to_a_saved_photo =
        photo_challenge_progress_snapshot_records
            .iter()
            .all(|record| {
                record.completed_by_photo_persistent_identifier.is_none_or(
                    |persistent_identifier| {
                        photo_persistent_identifiers
                            .binary_search_by_key(&persistent_identifier.0, |candidate| candidate.0)
                            .is_ok()
                    },
                )
            });

    let transport_circuit_persistent_identifiers = transport_snapshot_records
        .circuit_records
        .iter()
        .map(|record| record.persistent_identifier)
        .collect::<Vec<_>>();
    let every_transport_member_and_vehicle_refers_to_a_saved_circuit = transport_snapshot_records
        .circuit_member_records
        .iter()
        .map(|record| record.circuit_persistent_identifier)
        .chain(
            transport_snapshot_records
                .vehicle_records
                .iter()
                .map(|record| record.circuit_persistent_identifier),
        )
        .all(|persistent_identifier| {
            transport_circuit_persistent_identifiers
                .binary_search_by_key(&persistent_identifier.0, |candidate| candidate.0)
                .is_ok()
        });

    let mut imported_persistent_identifiers = photo_snapshot_records
        .iter()
        .map(|record| record.persistent_identifier)
        .chain(
            show_snapshot_records
                .stage_records
                .iter()
                .flat_map(|record| {
                    [
                        record.canopy_upgrade_persistent_identifier,
                        record.television_upgrade_persistent_identifier,
                    ]
                    .into_iter()
                    .flatten()
                }),
        )
        .chain(
            show_snapshot_records
                .scheduled_show_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .chain(
            show_snapshot_records
                .scheduled_break_records
                .iter()
                .map(|record| record.persistent_identifier),
        )
        .chain(
            transport_snapshot_records
                .circuit_records
                .iter()
                .map(|record| record.persistent_identifier)
                .chain(
                    transport_snapshot_records
                        .circuit_member_records
                        .iter()
                        .map(|record| record.persistent_identifier),
                )
                .chain(
                    transport_snapshot_records
                        .vehicle_records
                        .iter()
                        .map(|record| record.persistent_identifier),
                )
                .filter(|persistent_identifier| {
                    !persistent_identifier_exists(*persistent_identifier)
                }),
        )
        .chain(
            progression_snapshot_records
                .persistent_identifiers()
                .filter(|persistent_identifier| {
                    !progression_snapshot_application_parameters
                        .contains_persistent_identifier(*persistent_identifier)
                }),
        )
        .collect::<Vec<_>>();
    imported_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);
    let every_imported_persistent_identifier_is_valid_and_unique = imported_persistent_identifiers
        .iter()
        .enumerate()
        .all(|(index, persistent_identifier)| {
            !matches!(persistent_identifier.0, 0 | u64::MAX)
                && !persistent_identifier_exists(*persistent_identifier)
                && (index == 0
                    || imported_persistent_identifiers[index - 1] != *persistent_identifier)
        });

    if every_required_existing_entity_is_present
        && every_photo_challenge_completion_refers_to_a_saved_photo
        && every_transport_member_and_vehicle_refers_to_a_saved_circuit
        && every_imported_persistent_identifier_is_valid_and_unique
    {
        Ok(())
    } else {
        Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
    }
}

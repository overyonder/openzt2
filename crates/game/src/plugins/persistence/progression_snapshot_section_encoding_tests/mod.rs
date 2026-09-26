use openzt2_game_data::AssetId;

use crate::plugins::{
    progression::{
        adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy,
        fame_history_types::FameHistorySample, fame_types::Fame, rating_types::ZooRating,
    },
    world_spawn::persistent_id_types::PersistentId,
};

use super::{
    progression_snapshot_section_encoding::{
        append_encoded_progression_snapshot_section,
        decode_and_validate_progression_snapshot_section,
    },
    progression_snapshot_types::{
        EarnedProgressionAwardSnapshotRecord, ProgressionAwardDurationSnapshotRecord,
        ProgressionSnapshotRecords, ResearchAvailabilitySnapshotRecord,
        ResearchProjectSnapshotRecord, SpeciesAdoptionChanceSnapshotRecord,
    },
};

#[test]
fn progression_snapshot_section_round_trips_bounded_native_facts() {
    let original_progression_snapshot_records = ProgressionSnapshotRecords {
        fame: Fame {
            half_stars: 6,
            maximum_reached: 8,
            maximum_percent_reached: Some(83.25),
        },
        zoo_rating: ZooRating {
            animal_welfare_permille: 900,
            guest_satisfaction_permille: 800,
            education_permille: 700,
            variety_permille: 600,
            scenery_permille: 500,
            finance_permille: 400,
            cleanliness_permille: 300,
            overall_permille: 650,
            ..Default::default()
        },
        unlocked_catalogue_definition_count: 65,
        unlocked_catalogue_definition_bit_words: vec![u64::MAX, 1],
        scenario_award_points: 12,
        endangered_animal_birth_count: 7,
        animal_adoption_availability_policy: AnimalAdoptionAvailabilityPolicy::default(),
        fame_history_samples: vec![
            FameHistorySample {
                month_index: 0,
                half_stars: 2,
            },
            FameHistorySample {
                month_index: 1,
                half_stars: 4,
            },
        ],
        research_projects: vec![ResearchProjectSnapshotRecord {
            persistent_identifier: PersistentId(2),
            definition_identifier: AssetId([2; 16]),
            elapsed_simulation_ticks: 10,
            required_simulation_ticks: 20,
            paused: true,
        }],
        earned_progression_awards: vec![EarnedProgressionAwardSnapshotRecord {
            persistent_identifier: PersistentId(3),
            definition_identifier: AssetId([3; 16]),
            earned_simulation_tick: 44,
        }],
        award_condition_duration_progress: vec![ProgressionAwardDurationSnapshotRecord {
            persistent_identifier: PersistentId(4),
            award_definition_identifier: AssetId([4; 16]),
            award_condition_index: 1,
            satisfied_simulation_ticks: 12,
        }],
        research_project_availability: vec![ResearchAvailabilitySnapshotRecord {
            persistent_identifier: PersistentId(5),
            catalogue_item_identifier: AssetId([5; 16]),
            available: false,
            remaining_unlock_ticks: Some(30),
        }],
        species_adoption_chances: vec![SpeciesAdoptionChanceSnapshotRecord {
            persistent_identifier: PersistentId(6),
            definition_identifier: AssetId([6; 16]),
            adoption_chance_multiplier_permille: 500,
        }],
    };
    let mut progression_snapshot_section_bytes = Vec::new();
    append_encoded_progression_snapshot_section(
        &mut progression_snapshot_section_bytes,
        &original_progression_snapshot_records,
    );
    let decoded_progression_snapshot_records = decode_and_validate_progression_snapshot_section(
        &progression_snapshot_section_bytes,
        original_progression_snapshot_records.snapshot_record_count(),
    )
    .unwrap();
    assert_eq!(
        decoded_progression_snapshot_records.fame,
        original_progression_snapshot_records.fame
    );
    assert_eq!(
        decoded_progression_snapshot_records.zoo_rating,
        original_progression_snapshot_records.zoo_rating
    );
    assert_eq!(
        decoded_progression_snapshot_records.unlocked_catalogue_definition_bit_words,
        original_progression_snapshot_records.unlocked_catalogue_definition_bit_words
    );
    assert_eq!(
        decoded_progression_snapshot_records.fame_history_samples,
        original_progression_snapshot_records.fame_history_samples
    );
    assert_eq!(
        decoded_progression_snapshot_records.research_projects.len(),
        1
    );
    assert_eq!(
        decoded_progression_snapshot_records
            .earned_progression_awards
            .len(),
        1
    );
    assert_eq!(
        decoded_progression_snapshot_records
            .award_condition_duration_progress
            .len(),
        1
    );
    assert_eq!(
        decoded_progression_snapshot_records
            .research_project_availability
            .len(),
        1
    );
    assert_eq!(
        decoded_progression_snapshot_records
            .species_adoption_chances
            .len(),
        1
    );
}

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    progression::{
        adoption_and_content_availability_types::{
            AnimalAdoptionAvailabilityPolicy, SpeciesAdoptionChanceMultiplier,
        },
        award_and_progression_fact_types::{
            EarnedProgressionAward, ProgressionAwardConditionDurationProgress,
            ScenarioAwardPointTotal,
        },
        fame_history_types::FameHistory,
        fame_types::Fame,
        profile_challenge_types::TotalEndangeredAnimalBirthCount,
        rating_types::ZooRating,
        research_types::{
            ResearchAvailabilityUnlockCountdown, ResearchProject, ResearchProjectAvailability,
            ResearchProjectPaused,
        },
        unlock_types::UnlockedCatalogueDefinitionSet,
    },
    world_spawn::{persistent_id_types::PersistentId, world_membership_types::WorldRoot},
};

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_types::{
        EarnedProgressionAwardSnapshotRecord, ProgressionAwardDurationSnapshotRecord,
        ProgressionSnapshotRecords, ResearchAvailabilitySnapshotRecord,
        ResearchProjectSnapshotRecord, SpeciesAdoptionChanceSnapshotRecord,
    },
    progression_snapshot_validation::validate_progression_snapshot_record_invariants,
};

#[derive(SystemParam)]
pub(super) struct ProgressionSnapshotCaptureQueries<'w, 's> {
    fame: Res<'w, Fame>,
    zoo_rating: Res<'w, ZooRating>,
    unlocked_catalogue_definitions: Res<'w, UnlockedCatalogueDefinitionSet>,
    scenario_award_points: Res<'w, ScenarioAwardPointTotal>,
    endangered_animal_birth_count: Res<'w, TotalEndangeredAnimalBirthCount>,
    fame_history: Res<'w, FameHistory>,
    world_adoption_availability_policy:
        Query<'w, 's, Option<&'static AnimalAdoptionAvailabilityPolicy>, With<WorldRoot>>,
    research_projects: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ResearchProject,
            Option<&'static ResearchProjectPaused>,
        ),
    >,
    earned_progression_awards:
        Query<'w, 's, (&'static PersistentId, &'static EarnedProgressionAward)>,
    award_condition_duration_progress: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ProgressionAwardConditionDurationProgress,
        ),
    >,
    research_project_availability: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ResearchProjectAvailability,
            Option<&'static ResearchAvailabilityUnlockCountdown>,
        ),
    >,
    species_adoption_chances: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static SpeciesAdoptionChanceMultiplier,
        ),
    >,
    progression_entities_without_persistent_identifiers: Query<
        'w,
        's,
        (),
        (
            Or<(
                With<ResearchProject>,
                With<EarnedProgressionAward>,
                With<ProgressionAwardConditionDurationProgress>,
                With<ResearchProjectAvailability>,
                With<SpeciesAdoptionChanceMultiplier>,
            )>,
            Without<PersistentId>,
        ),
    >,
}

pub(super) fn capture_progression_snapshot_records_from_live_world(
    progression_snapshot_capture_queries: &ProgressionSnapshotCaptureQueries,
) -> Result<ProgressionSnapshotRecords, WorldSnapshotPersistenceFailure> {
    if !progression_snapshot_capture_queries
        .progression_entities_without_persistent_identifiers
        .is_empty()
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    let animal_adoption_availability_policy = progression_snapshot_capture_queries
        .world_adoption_availability_policy
        .single()
        .map(|optional_adoption_availability_policy| {
            optional_adoption_availability_policy
                .copied()
                .unwrap_or_default()
        })
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;

    let mut research_project_snapshot_records = progression_snapshot_capture_queries
        .research_projects
        .iter()
        .map(
            |(persistent_identifier, research_project, optional_paused_marker)| {
                ResearchProjectSnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    definition_identifier: research_project.definition,
                    elapsed_simulation_ticks: research_project.elapsed_ticks,
                    required_simulation_ticks: research_project.required_ticks,
                    paused: optional_paused_marker.is_some(),
                }
            },
        )
        .collect::<Vec<_>>();
    let mut earned_progression_award_snapshot_records = progression_snapshot_capture_queries
        .earned_progression_awards
        .iter()
        .map(|(persistent_identifier, earned_progression_award)| {
            EarnedProgressionAwardSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                definition_identifier: earned_progression_award.definition,
                earned_simulation_tick: earned_progression_award.earned_tick,
            }
        })
        .collect::<Vec<_>>();
    let mut award_duration_snapshot_records = progression_snapshot_capture_queries
        .award_condition_duration_progress
        .iter()
        .map(
            |(persistent_identifier, award_condition_duration_progress)| {
                ProgressionAwardDurationSnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    award_definition_identifier: award_condition_duration_progress.award,
                    award_condition_index: award_condition_duration_progress.condition_index,
                    satisfied_simulation_ticks: award_condition_duration_progress.satisfied_ticks,
                }
            },
        )
        .collect::<Vec<_>>();
    let mut research_availability_snapshot_records = progression_snapshot_capture_queries
        .research_project_availability
        .iter()
        .map(
            |(persistent_identifier, research_project_availability, optional_unlock_countdown)| {
                ResearchAvailabilitySnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    catalogue_item_identifier: research_project_availability.item,
                    available: research_project_availability.available,
                    remaining_unlock_ticks: optional_unlock_countdown
                        .map(|unlock_countdown| unlock_countdown.remaining_ticks),
                }
            },
        )
        .collect::<Vec<_>>();
    let mut species_adoption_chance_snapshot_records = progression_snapshot_capture_queries
        .species_adoption_chances
        .iter()
        .map(|(persistent_identifier, species_adoption_chance)| {
            SpeciesAdoptionChanceSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                definition_identifier: species_adoption_chance.definition,
                adoption_chance_multiplier_permille: species_adoption_chance.multiplier_permille,
            }
        })
        .collect::<Vec<_>>();

    research_project_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);
    earned_progression_award_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    award_duration_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);
    research_availability_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    species_adoption_chance_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);

    let progression_snapshot_records = ProgressionSnapshotRecords {
        fame: *progression_snapshot_capture_queries.fame,
        zoo_rating: *progression_snapshot_capture_queries.zoo_rating,
        unlocked_catalogue_definition_count: progression_snapshot_capture_queries
            .unlocked_catalogue_definitions
            .definition_count,
        unlocked_catalogue_definition_bit_words: progression_snapshot_capture_queries
            .unlocked_catalogue_definitions
            .words
            .clone(),
        scenario_award_points: progression_snapshot_capture_queries.scenario_award_points.0,
        endangered_animal_birth_count: progression_snapshot_capture_queries
            .endangered_animal_birth_count
            .0,
        animal_adoption_availability_policy,
        fame_history_samples: progression_snapshot_capture_queries
            .fame_history
            .samples()
            .to_vec(),
        research_projects: research_project_snapshot_records,
        earned_progression_awards: earned_progression_award_snapshot_records,
        award_condition_duration_progress: award_duration_snapshot_records,
        research_project_availability: research_availability_snapshot_records,
        species_adoption_chances: species_adoption_chance_snapshot_records,
    };
    validate_progression_snapshot_record_invariants(&progression_snapshot_records)?;
    Ok(progression_snapshot_records)
}

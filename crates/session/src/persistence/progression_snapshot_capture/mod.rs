use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_extinct_animals::extinct_animals::FossilSkeletonRewardTimer;
use openzt2_progression::progression::AwardDomainHistory;
use openzt2_progression::progression::{
    AuthoredAwardName, AuthoredAwardPoints, AwardGlobalVariables,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::extinct_animals::extinct_animal_observable_fact_types::FossilEducationProgress;
use openzt2_zoo_state::progression::award_and_progression_fact_types::ProgressionAwardPresentationAcknowledged;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::SpeciesAdoptionChanceMultiplier;
use openzt2_zoo_state::progression::award_and_progression_fact_types::EarnedProgressionAward;
use openzt2_zoo_state::progression::fame_history_types::FameHistory;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::progression::profile_challenge_types::TotalEndangeredAnimalBirthCount;
use openzt2_zoo_state::progression::rating_types::ZooRating;
use openzt2_zoo_state::progression::research_types::ResearchAvailabilityUnlockCountdown;
use openzt2_zoo_state::progression::research_types::ResearchProjectAvailability;
use openzt2_zoo_state::progression::research_types::ResearchProjectPaused;
use openzt2_zoo_state::progression::research_types::{
    ResearchProject, ResearchProjectPersistentId,
};
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_types::{
        EarnedProgressionAwardSnapshotRecord, FossilSkeletonRewardSnapshot,
        ProgressionSnapshotRecords, ResearchAvailabilitySnapshotRecord,
        ResearchProjectSnapshotRecord, SpeciesAdoptionChanceSnapshotRecord,
    },
    progression_snapshot_validation::validate_progression_snapshot_record_invariants,
};

#[derive(SystemParam)]
pub(super) struct ProgressionSnapshotCaptureQueries<'w, 's> {
    award_history: Query<'w, 's, &'static AwardDomainHistory, With<WorldRoot>>,
    fossil_education: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static WorldMember,
            &'static FossilEducationProgress,
            Option<&'static FossilSkeletonRewardTimer>,
        ),
    >,
    fixed_time: Res<'w, Time<Fixed>>,
    world_progression: Query<
        'w,
        's,
        (
            &'static Fame,
            &'static ZooRating,
            &'static UnlockedCatalogueDefinitionSet,
            Option<&'static AuthoredAwardPoints>,
            Option<&'static AwardGlobalVariables>,
            &'static TotalEndangeredAnimalBirthCount,
            &'static FameHistory,
            Option<&'static AnimalAdoptionAvailabilityPolicy>,
        ),
        With<WorldRoot>,
    >,
    research_projects: Query<
        'w,
        's,
        (
            &'static ResearchProjectPersistentId,
            &'static ResearchProject,
            Option<&'static ResearchProjectPaused>,
        ),
    >,
    earned_progression_awards: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static EarnedProgressionAward,
            Option<&'static AuthoredAwardName>,
            Has<ProgressionAwardPresentationAcknowledged>,
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
    research_projects_without_persistent_identifiers:
        Query<'w, 's, (), (With<ResearchProject>, Without<ResearchProjectPersistentId>)>,
    progression_entities_without_persistent_identifiers: Query<
        'w,
        's,
        (),
        (
            Or<(
                With<EarnedProgressionAward>,
                With<ResearchProjectAvailability>,
                With<SpeciesAdoptionChanceMultiplier>,
            )>,
            Without<PersistentId>,
        ),
    >,
}

pub(super) fn capture_progression_snapshot_records_from_live_world(
    progression_snapshot_capture_queries: &ProgressionSnapshotCaptureQueries,
    root: Entity,
) -> Result<ProgressionSnapshotRecords, WorldSnapshotPersistenceFailure> {
    if !progression_snapshot_capture_queries
        .progression_entities_without_persistent_identifiers
        .is_empty()
        || !progression_snapshot_capture_queries
            .research_projects_without_persistent_identifiers
            .is_empty()
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    let (
        fame,
        zoo_rating,
        unlocked,
        award_points,
        award_globals,
        birth_count,
        history,
        adoption_policy,
    ) = progression_snapshot_capture_queries
        .world_progression
        .get(root)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let animal_adoption_availability_policy = adoption_policy.copied().unwrap_or_default();

    let step = progression_snapshot_capture_queries.fixed_time.timestep();
    let nanos = |duration: std::time::Duration| {
        u64::try_from(duration.as_nanos())
            .map_err(|_| WorldSnapshotPersistenceFailure::CapacityExceeded)
    };
    let mut research_project_snapshot_records = progression_snapshot_capture_queries
        .research_projects
        .iter()
        .map(
            |(persistent_identifier, research_project, optional_paused_marker)| {
                let (required_simulation_ticks, elapsed_simulation_ticks) =
                    research_project.saved_ticks(step);
                Ok(ResearchProjectSnapshotRecord {
                    persistent_identifier: persistent_identifier.0,
                    definition_identifier: research_project.definition,
                    elapsed_simulation_ticks,
                    required_simulation_ticks,
                    paused: optional_paused_marker.is_some(),
                    timer_ns: Some((
                        nanos(research_project.timer.duration())?,
                        nanos(research_project.timer.elapsed())?,
                    )),
                })
            },
        )
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let mut earned_progression_award_snapshot_records = progression_snapshot_capture_queries
        .earned_progression_awards
        .iter()
        .map(|(persistent_identifier, earned_progression_award, _, _)| {
            EarnedProgressionAwardSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                definition_identifier: earned_progression_award.definition,
                earned_simulation_tick: earned_progression_award.earned_tick,
            }
        })
        .collect::<Vec<_>>();
    let mut research_availability_snapshot_records = progression_snapshot_capture_queries
        .research_project_availability
        .iter()
        .map(
            |(persistent_identifier, research_project_availability, optional_unlock_countdown)| {
                Ok(ResearchAvailabilitySnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    catalogue_item_identifier: research_project_availability.item,
                    available: research_project_availability.available,
                    remaining_unlock_ticks: optional_unlock_countdown
                        .map(|countdown| countdown.remaining_ticks(step)),
                    unlock_timer_ns: optional_unlock_countdown
                        .map(|countdown| {
                            Ok::<_, WorldSnapshotPersistenceFailure>((
                                nanos(countdown.timer.duration())?,
                                nanos(countdown.timer.elapsed())?,
                            ))
                        })
                        .transpose()?,
                })
            },
        )
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
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
    research_availability_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    species_adoption_chance_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);

    let progression_snapshot_records = ProgressionSnapshotRecords {
        fame: *fame,
        zoo_rating: *zoo_rating,
        unlocked_catalogue_definition_count: unlocked.definition_count,
        unlocked_catalogue_definition_bit_words: unlocked.words.clone(),
        scenario_award_points: award_points.map_or(0, |points| points.0),
        award_history: progression_snapshot_capture_queries
            .award_history
            .get(root)
            .ok()
            .map(|history| AwardDomainHistory {
                released_species: history.released_species.clone(),
                extinct_released_species: history.extinct_released_species.clone(),
                dinosaur_tranquilizations: history.dinosaur_tranquilizations,
                perfect_training_sessions: history.perfect_training_sessions,
                cured_diseases: history.cured_diseases,
                super_clones: history.super_clones,
            }),
        fossil_education: progression_snapshot_capture_queries
            .fossil_education
            .iter()
            .filter(|(_, member, _, _)| member.root == root)
            .map(|(id, _, progress, _)| (id.0, progress.bone_level))
            .collect(),
        fossil_skeleton_rewards: progression_snapshot_capture_queries
            .fossil_education
            .iter()
            .filter(|(_, member, _, _)| member.root == root)
            .filter_map(|(id, _, _, reward)| {
                let reward = reward?;
                Some((
                    id.0,
                    FossilSkeletonRewardSnapshot {
                        remaining_ns: u64::try_from(reward.timer.remaining().as_nanos())
                            .unwrap_or(u64::MAX),
                        reward: reward.reward,
                        message: reward.message,
                    },
                ))
            })
            .collect(),
        award_globals: award_globals.map_or_else(Default::default, |globals| globals.0.clone()),
        award_presentation: progression_snapshot_capture_queries
            .earned_progression_awards
            .iter()
            .filter_map(|(id, _, name, seen)| {
                (name.is_some() || seen).then_some((id.0, (name.map(|name| name.0), seen)))
            })
            .collect(),
        endangered_animal_birth_count: birth_count.0,
        animal_adoption_availability_policy,
        fame_history_samples: history.samples().to_vec(),
        research_projects: research_project_snapshot_records,
        earned_progression_awards: earned_progression_award_snapshot_records,
        award_condition_duration_progress: Vec::new(),
        research_project_availability: research_availability_snapshot_records,
        species_adoption_chances: species_adoption_chance_snapshot_records,
    };
    validate_progression_snapshot_record_invariants(&progression_snapshot_records)?;
    Ok(progression_snapshot_records)
}

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    progression::{
        adoption_and_content_availability_types::SpeciesAdoptionChanceMultiplier,
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
    world_spawn::{
        persistent_id_types::PersistentId, persistent_id_types::PersistentIdAllocator,
        world_membership_types::WorldMember,
    },
};

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_types::ProgressionSnapshotRecords,
};

#[derive(SystemParam)]
pub(super) struct ProgressionSnapshotApplicationParameters<'w, 's> {
    fame: ResMut<'w, Fame>,
    zoo_rating: ResMut<'w, ZooRating>,
    unlocked_catalogue_definitions: ResMut<'w, UnlockedCatalogueDefinitionSet>,
    scenario_award_points: ResMut<'w, ScenarioAwardPointTotal>,
    endangered_animal_birth_count: ResMut<'w, TotalEndangeredAnimalBirthCount>,
    fame_history: ResMut<'w, FameHistory>,
    progression_entities_with_persistent_identifiers: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            Option<&'static ResearchProject>,
            Option<&'static EarnedProgressionAward>,
            Option<&'static ProgressionAwardConditionDurationProgress>,
            Option<&'static ResearchProjectAvailability>,
            Option<&'static SpeciesAdoptionChanceMultiplier>,
        ),
        Or<(
            With<ResearchProject>,
            With<EarnedProgressionAward>,
            With<ProgressionAwardConditionDurationProgress>,
            With<ResearchProjectAvailability>,
            With<SpeciesAdoptionChanceMultiplier>,
        )>,
    >,
}

impl ProgressionSnapshotApplicationParameters<'_, '_> {
    pub(super) fn contains_persistent_identifier(
        &self,
        persistent_identifier: PersistentId,
    ) -> bool {
        self.progression_entities_with_persistent_identifiers
            .iter()
            .any(|(_, candidate_persistent_identifier, ..)| {
                *candidate_persistent_identifier == persistent_identifier
            })
    }
}

pub(super) fn apply_progression_snapshot_records_to_live_world(
    commands: &mut Commands,
    progression_snapshot_records: ProgressionSnapshotRecords,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    progression_snapshot_application_parameters: &mut ProgressionSnapshotApplicationParameters,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut snapshot_progression_persistent_identifiers = progression_snapshot_records
        .persistent_identifiers()
        .collect::<Vec<_>>();
    snapshot_progression_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);

    if !progression_snapshot_application_parameters
        .fame_history
        .replace_saved(progression_snapshot_records.fame_history_samples)
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    *progression_snapshot_application_parameters.fame = progression_snapshot_records.fame;
    *progression_snapshot_application_parameters.zoo_rating =
        progression_snapshot_records.zoo_rating;
    progression_snapshot_application_parameters
        .unlocked_catalogue_definitions
        .definition_count = progression_snapshot_records.unlocked_catalogue_definition_count;
    progression_snapshot_application_parameters
        .unlocked_catalogue_definitions
        .words = progression_snapshot_records.unlocked_catalogue_definition_bit_words;
    progression_snapshot_application_parameters
        .scenario_award_points
        .0 = progression_snapshot_records.scenario_award_points;
    progression_snapshot_application_parameters
        .endangered_animal_birth_count
        .0 = progression_snapshot_records.endangered_animal_birth_count;
    commands
        .entity(world_root_entity)
        .insert(progression_snapshot_records.animal_adoption_availability_policy);

    for (progression_entity, persistent_identifier, ..) in
        &progression_snapshot_application_parameters
            .progression_entities_with_persistent_identifiers
    {
        if snapshot_progression_persistent_identifiers
            .binary_search_by_key(&persistent_identifier.0, |snapshot_persistent_identifier| {
                snapshot_persistent_identifier.0
            })
            .is_err()
        {
            commands.entity(progression_entity).despawn();
        }
    }

    for research_project_snapshot_record in progression_snapshot_records.research_projects {
        let research_project_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            research_project_snapshot_record.persistent_identifier,
        )?;
        commands
            .entity(research_project_entity)
            .insert(ResearchProject {
                definition: research_project_snapshot_record.definition_identifier,
                elapsed_ticks: research_project_snapshot_record.elapsed_simulation_ticks,
                required_ticks: research_project_snapshot_record.required_simulation_ticks,
            });
        if research_project_snapshot_record.paused {
            commands
                .entity(research_project_entity)
                .insert(ResearchProjectPaused);
        } else {
            commands
                .entity(research_project_entity)
                .remove::<ResearchProjectPaused>();
        }
    }
    for earned_award_snapshot_record in progression_snapshot_records.earned_progression_awards {
        let earned_award_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            earned_award_snapshot_record.persistent_identifier,
        )?;
        commands
            .entity(earned_award_entity)
            .insert(EarnedProgressionAward {
                definition: earned_award_snapshot_record.definition_identifier,
                earned_tick: earned_award_snapshot_record.earned_simulation_tick,
            });
    }
    for award_duration_snapshot_record in
        progression_snapshot_records.award_condition_duration_progress
    {
        let award_duration_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            award_duration_snapshot_record.persistent_identifier,
        )?;
        commands
            .entity(award_duration_entity)
            .insert(ProgressionAwardConditionDurationProgress {
                award: award_duration_snapshot_record.award_definition_identifier,
                condition_index: award_duration_snapshot_record.award_condition_index,
                satisfied_ticks: award_duration_snapshot_record.satisfied_simulation_ticks,
            });
    }
    for research_availability_snapshot_record in
        progression_snapshot_records.research_project_availability
    {
        let research_availability_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            research_availability_snapshot_record.persistent_identifier,
        )?;
        commands
            .entity(research_availability_entity)
            .insert(ResearchProjectAvailability {
                item: research_availability_snapshot_record.catalogue_item_identifier,
                available: research_availability_snapshot_record.available,
            });
        if let Some(remaining_unlock_ticks) =
            research_availability_snapshot_record.remaining_unlock_ticks
        {
            commands.entity(research_availability_entity).insert(
                ResearchAvailabilityUnlockCountdown {
                    item: research_availability_snapshot_record.catalogue_item_identifier,
                    remaining_ticks: remaining_unlock_ticks,
                },
            );
        } else {
            commands
                .entity(research_availability_entity)
                .remove::<ResearchAvailabilityUnlockCountdown>();
        }
    }
    for species_adoption_chance_snapshot_record in
        progression_snapshot_records.species_adoption_chances
    {
        let species_adoption_chance_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            species_adoption_chance_snapshot_record.persistent_identifier,
        )?;
        commands
            .entity(species_adoption_chance_entity)
            .insert(SpeciesAdoptionChanceMultiplier {
                definition: species_adoption_chance_snapshot_record.definition_identifier,
                multiplier_permille: species_adoption_chance_snapshot_record
                    .adoption_chance_multiplier_permille,
            });
    }
    Ok(())
}

fn find_existing_or_spawn_progression_snapshot_entity(
    commands: &mut Commands,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    progression_entities_with_persistent_identifiers: &Query<
        (
            Entity,
            &PersistentId,
            Option<&ResearchProject>,
            Option<&EarnedProgressionAward>,
            Option<&ProgressionAwardConditionDurationProgress>,
            Option<&ResearchProjectAvailability>,
            Option<&SpeciesAdoptionChanceMultiplier>,
        ),
        Or<(
            With<ResearchProject>,
            With<EarnedProgressionAward>,
            With<ProgressionAwardConditionDurationProgress>,
            With<ResearchProjectAvailability>,
            With<SpeciesAdoptionChanceMultiplier>,
        )>,
    >,
    persistent_identifier: PersistentId,
) -> Result<Entity, WorldSnapshotPersistenceFailure> {
    if let Some(existing_progression_entity) = progression_entities_with_persistent_identifiers
        .iter()
        .find_map(
            |(progression_entity, candidate_persistent_identifier, ..)| {
                (*candidate_persistent_identifier == persistent_identifier)
                    .then_some(progression_entity)
            },
        )
    {
        return Ok(existing_progression_entity);
    }
    persistent_identifier_allocator
        .reserve_imported(world_root_entity, persistent_identifier)
        .map_err(|_| WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier)?;
    Ok(commands
        .spawn((
            persistent_identifier,
            WorldMember {
                root: world_root_entity,
            },
        ))
        .id())
}

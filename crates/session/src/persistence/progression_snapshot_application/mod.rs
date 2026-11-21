use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::SpeciesAdoptionChanceMultiplier;
use openzt2_zoo_state::progression::award_and_progression_fact_types::EarnedProgressionAward;
use openzt2_zoo_state::progression::award_and_progression_fact_types::ProgressionAwardConditionDurationProgress;
use openzt2_zoo_state::progression::award_and_progression_fact_types::ScenarioAwardPointTotal;
use openzt2_zoo_state::progression::fame_history_types::FameHistory;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::progression::profile_challenge_types::TotalEndangeredAnimalBirthCount;
use openzt2_zoo_state::progression::rating_types::ZooRating;
use openzt2_zoo_state::progression::research_types::ResearchAvailabilityUnlockCountdown;
use openzt2_zoo_state::progression::research_types::ResearchProject;
use openzt2_zoo_state::progression::research_types::ResearchProjectAvailability;
use openzt2_zoo_state::progression::research_types::{
    ResearchItemInWorld, ResearchProjectPaused, ResearchProjectPersistentId, WorldResearchItems,
};
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;

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
    research_items: Query<'w, 's, &'static WorldResearchItems>,
    active_research: Query<
        'w,
        's,
        (
            &'static ResearchProjectPersistentId,
            &'static mut ResearchProject,
        ),
    >,
    fame_history: ResMut<'w, FameHistory>,
    progression_entities_with_persistent_identifiers: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            Option<&'static EarnedProgressionAward>,
            Option<&'static ProgressionAwardConditionDurationProgress>,
            Option<&'static ResearchProjectAvailability>,
            Option<&'static SpeciesAdoptionChanceMultiplier>,
        ),
        Or<(
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
            || self
                .active_research
                .iter()
                .any(|(id, _)| id.0 == persistent_identifier)
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
    let mut restored_research_items = std::collections::BTreeMap::new();
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
        let definition = research_availability_snapshot_record.catalogue_item_identifier;
        restored_research_items.insert(definition, research_availability_entity);
        commands.entity(research_availability_entity).insert((
            ResearchItemInWorld(world_root_entity),
            ResearchProjectAvailability {
                item: definition,
                available: research_availability_snapshot_record.available,
            },
        ));
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
    for project in progression_snapshot_records.research_projects {
        let definition = project.definition_identifier;
        let item = restored_research_items
            .get(&definition)
            .copied()
            .or_else(|| {
                progression_snapshot_application_parameters
                    .research_items
                    .get(world_root_entity)
                    .ok()?
                    .definitions
                    .get(&definition)
                    .copied()
            });
        let item = if let Some(item) = item {
            item
        } else {
            let id = persistent_identifier_allocator
                .allocate(world_root_entity)
                .map_err(|_| {
                    WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier
                })?;
            commands
                .spawn((
                    id,
                    WorldMember {
                        root: world_root_entity,
                    },
                    ResearchItemInWorld(world_root_entity),
                    ResearchProjectAvailability {
                        item: definition,
                        available: true,
                    },
                ))
                .id()
        };
        let current = progression_snapshot_application_parameters
            .active_research
            .get_mut(item)
            .ok();
        let progress = ResearchProject {
            definition,
            elapsed_ticks: project.elapsed_simulation_ticks,
            required_ticks: project.required_simulation_ticks,
        };
        let mut entity = commands.entity(item);
        if let Some((id, mut state)) = current {
            state.set_if_neq(progress);
            if id.0 != project.persistent_identifier {
                persistent_identifier_allocator
                    .reserve_imported(world_root_entity, project.persistent_identifier)
                    .map_err(|_| {
                        WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier
                    })?;
                entity.insert(ResearchProjectPersistentId(project.persistent_identifier));
            }
        } else {
            persistent_identifier_allocator
                .reserve_imported(world_root_entity, project.persistent_identifier)
                .map_err(|_| {
                    WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier
                })?;
            entity.insert((
                ResearchProjectPersistentId(project.persistent_identifier),
                progress,
            ));
        }
        if project.paused {
            entity.insert_if_new(ResearchProjectPaused);
        } else {
            entity.remove::<ResearchProjectPaused>();
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
            Option<&EarnedProgressionAward>,
            Option<&ProgressionAwardConditionDurationProgress>,
            Option<&ResearchProjectAvailability>,
            Option<&SpeciesAdoptionChanceMultiplier>,
        ),
        Or<(
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

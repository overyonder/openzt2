use super::slot_io::restored_component_values::restore_optional_component;
use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
use openzt2_progression::progression::{
    AuthoredAwardName, AuthoredAwardPoints, AwardGlobalVariables,
};
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_zoo_state::progression::award_and_progression_fact_types::ProgressionAwardPresentationAcknowledged;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::{
    AnimalAdoptionAvailabilityPolicy, SpeciesAdoptionChanceMultiplier,
};
use openzt2_zoo_state::progression::award_and_progression_fact_types::EarnedProgressionAward;
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
    progression_snapshot_types::{ProgressionSnapshotRecords, ResearchProjectSnapshotRecord},
};

#[derive(SystemParam)]
pub(super) struct ProgressionSnapshotApplicationParameters<'w, 's> {
    fixed_time: Res<'w, Time<Fixed>>,
    world_progression: Query<
        'w,
        's,
        (
            &'static mut Fame,
            &'static mut ZooRating,
            &'static mut UnlockedCatalogueDefinitionSet,
            Option<&'static mut AuthoredAwardPoints>,
            Option<&'static mut AwardGlobalVariables>,
            &'static mut TotalEndangeredAnimalBirthCount,
            &'static mut FameHistory,
        ),
    >,
    adoption_policy: Query<'w, 's, &'static mut AnimalAdoptionAvailabilityPolicy>,
    active_research: Query<
        'w,
        's,
        (
            &'static mut ResearchProjectPersistentId,
            &'static mut ResearchProject,
        ),
    >,
    progression_entities_with_persistent_identifiers:
        Query<'w, 's, ProgressionEntityValues, ProgressionValueFilter>,
}

type ProgressionValueFilter = Or<(
    With<EarnedProgressionAward>,
    With<ResearchProjectAvailability>,
    With<SpeciesAdoptionChanceMultiplier>,
)>;

#[derive(QueryData)]
#[query_data(mutable)]
struct ProgressionEntityValues {
    entity: Entity,
    id: &'static PersistentId,
    award: Option<&'static mut EarnedProgressionAward>,
    award_name: Option<&'static mut AuthoredAwardName>,
    award_seen: Has<ProgressionAwardPresentationAcknowledged>,
    availability: Option<&'static mut ResearchProjectAvailability>,
    unlock_countdown: Option<&'static mut ResearchAvailabilityUnlockCountdown>,
    adoption_chance: Option<&'static mut SpeciesAdoptionChanceMultiplier>,
    has_research_owner: Has<ResearchItemInWorld>,
}

impl ProgressionSnapshotApplicationParameters<'_, '_> {
    pub(super) fn contains_persistent_identifier(
        &self,
        persistent_identifier: PersistentId,
    ) -> bool {
        self.progression_entities_with_persistent_identifiers
            .iter()
            .any(|row| *row.id == persistent_identifier)
            || self
                .active_research
                .iter()
                .any(|(id, _)| id.0 == persistent_identifier)
    }
}

#[derive(Component)]
pub(super) struct SavedResearchProjects {
    projects: Vec<ResearchProjectSnapshotRecord>,
    available_items: std::collections::BTreeMap<openzt2_game_data::AssetId, PersistentId>,
}

pub(super) fn apply_progression_snapshot_records_to_live_world(
    commands: &mut Commands,
    progression_snapshot_records: ProgressionSnapshotRecords,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    identities: &PersistentIdIndex,
    progression_snapshot_application_parameters: &mut ProgressionSnapshotApplicationParameters,
) -> Result<SavedResearchProjects, WorldSnapshotPersistenceFailure> {
    let step = progression_snapshot_application_parameters
        .fixed_time
        .timestep();
    let mut snapshot_progression_persistent_identifiers = progression_snapshot_records
        .persistent_identifiers()
        .collect::<Vec<_>>();
    snapshot_progression_persistent_identifiers
        .sort_unstable_by_key(|persistent_identifier| persistent_identifier.0);

    let (
        mut fame,
        mut rating,
        mut unlocked,
        award_points,
        award_globals,
        mut birth_count,
        mut history,
    ) = progression_snapshot_application_parameters
        .world_progression
        .get_mut(world_root_entity)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if !history.replace_saved(progression_snapshot_records.fame_history_samples) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    fame.set_if_neq(progression_snapshot_records.fame);
    rating.set_if_neq(progression_snapshot_records.zoo_rating);
    unlocked.set_if_neq(UnlockedCatalogueDefinitionSet {
        definition_count: progression_snapshot_records.unlocked_catalogue_definition_count,
        words: progression_snapshot_records.unlocked_catalogue_definition_bit_words,
    });
    let mut root = commands.entity(world_root_entity);
    if let Some(mut points) = award_points {
        if points.0 != progression_snapshot_records.scenario_award_points {
            points.0 = progression_snapshot_records.scenario_award_points;
        }
    } else {
        root.insert(AuthoredAwardPoints(
            progression_snapshot_records.scenario_award_points,
        ));
    }
    if let Some(mut globals) = award_globals {
        if globals.0 != progression_snapshot_records.award_globals {
            globals.0 = progression_snapshot_records.award_globals;
        }
    } else if !progression_snapshot_records.award_globals.is_empty() {
        root.insert(AwardGlobalVariables(
            progression_snapshot_records.award_globals,
        ));
    }
    birth_count.set_if_neq(TotalEndangeredAnimalBirthCount(
        progression_snapshot_records.endangered_animal_birth_count,
    ));
    restore_optional_component(
        &mut commands.entity(world_root_entity),
        progression_snapshot_application_parameters
            .adoption_policy
            .get_mut(world_root_entity)
            .ok(),
        Some(progression_snapshot_records.animal_adoption_availability_policy),
        false,
    );

    for row in &progression_snapshot_application_parameters
        .progression_entities_with_persistent_identifiers
    {
        if identities.get(world_root_entity, *row.id) == Some(row.entity)
            && snapshot_progression_persistent_identifiers
                .binary_search_by_key(&row.id.0, |id| id.0)
                .is_err()
        {
            commands.entity(row.entity).despawn();
        }
    }
    for earned_award_snapshot_record in progression_snapshot_records.earned_progression_awards {
        let earned_award_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            identities,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            earned_award_snapshot_record.persistent_identifier,
        )?;
        let (name, seen) = progression_snapshot_records
            .award_presentation
            .get(&earned_award_snapshot_record.persistent_identifier.0)
            .copied()
            .unwrap_or_default();
        let (existing, existing_name, existing_seen) = progression_snapshot_application_parameters
            .progression_entities_with_persistent_identifiers
            .get_mut(earned_award_entity)
            .map(|row| (row.award, row.award_name, row.award_seen))
            .unwrap_or_default();
        let mut award = commands.entity(earned_award_entity);
        restore_optional_component(&mut award, existing_name, name.map(AuthoredAwardName), true);
        match (existing_seen, seen) {
            (false, true) => {
                award.insert(ProgressionAwardPresentationAcknowledged);
            }
            (true, false) => {
                award.remove::<ProgressionAwardPresentationAcknowledged>();
            }
            _ => {}
        }
        restore_optional_component(
            &mut commands.entity(earned_award_entity),
            existing,
            Some(EarnedProgressionAward {
                definition: earned_award_snapshot_record.definition_identifier,
                earned_tick: earned_award_snapshot_record.earned_simulation_tick,
            }),
            false,
        );
    }
    let mut available_items = std::collections::BTreeMap::new();
    for research_availability_snapshot_record in
        progression_snapshot_records.research_project_availability
    {
        let research_availability_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            identities,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            research_availability_snapshot_record.persistent_identifier,
        )?;
        let definition = research_availability_snapshot_record.catalogue_item_identifier;
        available_items.insert(
            definition,
            research_availability_snapshot_record.persistent_identifier,
        );
        let existing = progression_snapshot_application_parameters
            .progression_entities_with_persistent_identifiers
            .get_mut(research_availability_entity)
            .ok();
        let (availability, countdown, owned) = existing.map_or((None, None, false), |row| {
            (
                row.availability,
                row.unlock_countdown,
                row.has_research_owner,
            )
        });
        let mut entity = commands.entity(research_availability_entity);
        if !owned {
            entity.insert(ResearchItemInWorld(world_root_entity));
        }
        restore_optional_component(
            &mut entity,
            availability,
            Some(ResearchProjectAvailability {
                item: definition,
                available: research_availability_snapshot_record.available,
            }),
            false,
        );
        let saved_countdown = research_availability_snapshot_record
            .remaining_unlock_ticks
            .map(|remaining_ticks| ResearchAvailabilityUnlockCountdown {
                item: definition,
                timer: Timer::new(duration_for_ticks(step, remaining_ticks), TimerMode::Once),
            });
        restore_optional_component(&mut entity, countdown, saved_countdown, true);
    }
    for species_adoption_chance_snapshot_record in
        progression_snapshot_records.species_adoption_chances
    {
        let species_adoption_chance_entity = find_existing_or_spawn_progression_snapshot_entity(
            commands,
            world_root_entity,
            persistent_identifier_allocator,
            identities,
            &progression_snapshot_application_parameters
                .progression_entities_with_persistent_identifiers,
            species_adoption_chance_snapshot_record.persistent_identifier,
        )?;
        let existing = progression_snapshot_application_parameters
            .progression_entities_with_persistent_identifiers
            .get_mut(species_adoption_chance_entity)
            .ok()
            .and_then(|row| row.adoption_chance);
        restore_optional_component(
            &mut commands.entity(species_adoption_chance_entity),
            existing,
            Some(SpeciesAdoptionChanceMultiplier {
                definition: species_adoption_chance_snapshot_record.definition_identifier,
                multiplier_permille: species_adoption_chance_snapshot_record
                    .adoption_chance_multiplier_permille,
            }),
            false,
        );
    }
    Ok(SavedResearchProjects {
        projects: progression_snapshot_records.research_projects,
        available_items,
    })
}

pub(super) fn restore_saved_research_projects(
    commands: &mut Commands,
    saved: &mut SavedResearchProjects,
    world_root_entity: Entity,
    identities: &PersistentIdIndex,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    research_items: &Query<&WorldResearchItems>,
    active_research: &mut Query<(&mut ResearchProjectPersistentId, &mut ResearchProject)>,
    step: std::time::Duration,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    for project in std::mem::take(&mut saved.projects) {
        let definition = project.definition_identifier;
        let item = saved
            .available_items
            .get(&definition)
            .and_then(|id| identities.get(world_root_entity, *id))
            .or_else(|| {
                research_items
                    .get(world_root_entity)
                    .ok()?
                    .definitions
                    .get(&definition)
                    .copied()
            });
        let item = if let Some(item) = item {
            item
        } else {
            let id = persistent_identifier_allocator.allocate().map_err(|_| {
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
        let current = active_research.get_mut(item).ok();
        let mut timer = Timer::new(
            duration_for_ticks(step, project.required_simulation_ticks),
            TimerMode::Once,
        );
        timer.set_elapsed(duration_for_ticks(step, project.elapsed_simulation_ticks));
        let progress = ResearchProject { definition, timer };
        let mut entity = commands.entity(item);
        if let Some((mut id, mut state)) = current {
            state.set_if_neq(progress);
            if id.0 != project.persistent_identifier {
                persistent_identifier_allocator
                    .reserve_imported(project.persistent_identifier)
                    .map_err(|_| {
                        WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier
                    })?;
                id.0 = project.persistent_identifier;
            }
        } else {
            persistent_identifier_allocator
                .reserve_imported(project.persistent_identifier)
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
    Ok(())
}

fn duration_for_ticks(step: std::time::Duration, ticks: u64) -> std::time::Duration {
    let ns = step.as_nanos().saturating_mul(u128::from(ticks));
    std::time::Duration::new(
        u64::try_from(ns / 1_000_000_000).unwrap_or(u64::MAX),
        (ns % 1_000_000_000) as u32,
    )
}

fn find_existing_or_spawn_progression_snapshot_entity(
    commands: &mut Commands,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    identities: &PersistentIdIndex,
    progression_entities: &Query<ProgressionEntityValues, ProgressionValueFilter>,
    persistent_identifier: PersistentId,
) -> Result<Entity, WorldSnapshotPersistenceFailure> {
    if let Some(entity) = identities.get(world_root_entity, persistent_identifier) {
        return progression_entities
            .contains(entity)
            .then_some(entity)
            .ok_or(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
    }
    persistent_identifier_allocator
        .reserve_imported(persistent_identifier)
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

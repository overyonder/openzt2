use bevy::prelude::{Commands, Entity, Handle, IRect};
use std::collections::BTreeMap;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        progression_snapshot_application::ProgressionSnapshotApplicationParameters,
    },
    animal_adoption_offer_inventory_snapshot_section::validate_animal_adoption_offer_inventory_against_loaded_authored_content,
    loaded_snapshot_reference_validation::validate_loaded_snapshot_references_and_imported_persistent_identifiers,
    supported_snapshot_section_validation::DecodedSnapshot,
    tank_snapshot_application::apply_tank_snapshot_records_to_live_world,
    world_snapshot_application_system_parameters::WorldSnapshotApplicationState,
};

pub(super) struct SnapshotSpawns {
    pub(super) retired_fence_bounds: Option<IRect>,
}

pub(super) fn restore_snapshot_sections(
    commands: &mut Commands,
    snapshot: &mut DecodedSnapshot,
    source: openzt2_interface_state::persistence::saved_game_source_types::SavedGameSource,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    world_snapshot_application_state: WorldSnapshotApplicationState,
    progression_snapshot_application_parameters: &ProgressionSnapshotApplicationParameters,
    retained_maintenance_prefabs: &mut Vec<
        bevy::prelude::Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    >,
    behavior_documents: &mut Vec<
        Handle<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>,
    >,
) -> Result<Option<SnapshotSpawns>, WorldSnapshotPersistenceFailure> {
    let DecodedSnapshot {
        world: world_snapshot_record,
        behavior: behavior_snapshot_records,
        sections,
        ..
    } = snapshot;
    let WorldSnapshotApplicationState {
        persistent_ids,
        mut behavior,
        topology,
        scenario,
        placements,
        placement_models,
        baseline_waste,
        prefabs,
        asset_server,
        baseline_animals,
        species_assets,
        active_species,
        world_definition_assets,
        active_world_definitions,
        entities_with_persistent_identifiers,
        entities_with_definition_identifiers,
        mut show_stages_with_mutable_snapshot_state,
        mut photo_challenges_with_mutable_progress,
        tank_persistent_identifiers,
        mut restored_tanks,
        photo_album_persistent_identifiers,
    } = world_snapshot_application_state;

    let Some(species) = active_species.get(&species_assets) else {
        return if active_species.catalogue_load_is_pending(&asset_server) {
            Ok(None)
        } else {
            Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)
        };
    };

    let Some(records) = sections.as_ref() else {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    };
    let baseline_topology = if let Some(saved) = &records.topology {
        topology
            .baseline_entities(world_root_entity, saved.includes_paths)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let world_definitions = active_world_definitions
        .get(&world_definition_assets)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let mut scenario_snapshot = records.scenario.clone();
    let mut runtime_continuation = records.runtime.clone();
    if matches!(
        source,
        openzt2_interface_state::persistence::saved_game_source_types::SavedGameSource::Original(_)
    ) {
        let original_next = world_snapshot_record
            .source_document
            .as_ref()
            .and_then(|source| {
                source
                    .document
                    .find_starting_zoo(world_snapshot_record.starting_point_definition_identifier)
            })
            .and_then(|start| start.next_persistent_identifier)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        if persistent_identifier_allocator.next() < original_next {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let next_identifier = super::scenario_snapshot_section::original_saved_game_conversion::assign_original_scenario_objective_identifiers(
            &mut scenario_snapshot,
            world_root_entity,
            persistent_identifier_allocator,
        )?;
        let runtime = runtime_continuation
            .as_mut()
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        runtime.next_persistent_identifier = next_identifier;
    }
    let placement_records = records.placements.as_deref().unwrap_or_default();
    let baseline_placements = if records.placements.is_some() {
        super::placement_snapshot_section::baseline_placement_entities(
            &placements,
            world_definitions,
            world_root_entity,
        )
    } else {
        Vec::new()
    };
    let Some(resolved_placement_prefabs) =
        super::placement_snapshot_section::resolve_placement_prefabs(
            placement_records,
            world_definitions,
            &prefabs,
            &placement_models,
            &asset_server,
            retained_maintenance_prefabs,
        )?
    else {
        return Ok(None);
    };
    if let Some(inventory) = records.adoption.as_ref() {
        validate_animal_adoption_offer_inventory_against_loaded_authored_content(
            inventory,
            species,
            world_definitions
                .animal_adoption_offer_configuration()
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?,
            records.progression.fame,
        )?;
    }
    let cadence = openzt2_simulation_time::simulation_time_world_lifecycle::zoo_day_cadence_from_world_definitions(world_definitions)
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let offset = world_snapshot_record
        .source_document
        .as_ref()
        .and_then(|source| {
            let start = source
                .document
                .find_starting_zoo(world_snapshot_record.starting_point_definition_identifier)?;
            let clock = cadence.clock_at(start.start_tick, Default::default())?;
            Some(i64::from(start.absolute_day) - i64::from(clock.absolute_day))
        });
    records
        .time
        .validate_against_timing(world_definitions.timing(), cadence, offset)?;
    let Some(resolved_waste_prefabs) =
        super::maintenance_snapshot_application::resolve_loose_waste_prefabs(
            &records.maintenance.waste,
            world_definitions,
            &prefabs,
            &asset_server,
            retained_maintenance_prefabs,
        )?
    else {
        return Ok(None);
    };
    let Some(resolved_staff_prefabs) =
        super::staff_snapshot_application::resolve_staff_body_prefabs(
            &records.staff,
            world_definitions,
            &prefabs,
            &asset_server,
            retained_maintenance_prefabs,
        )?
    else {
        return Ok(None);
    };
    let baseline_staff = super::staff_snapshot_application::baseline_staff_entities(
        world_definitions,
        &entities_with_definition_identifiers,
    );
    let resolved_animals = super::animal_snapshot_application::resolve_saved_animal_components(
        &records.animals,
        species,
        openzt2_simulation_time::simulation_time_world_lifecycle::zoo_day_cadence_from_world_definitions(world_definitions)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?.ticks_per_zoo_day(),
    )?;
    let baseline_scenario = if scenario_snapshot.is_some() {
        scenario.replaced_entities(world_root_entity)
    } else {
        Vec::new()
    };
    let (mut maximum_persistent_identifier, mut complete_persistent_identifiers) =
        validate_loaded_snapshot_references_and_imported_persistent_identifiers(
            scenario_snapshot.as_ref(),
            &records.animals,
            &baseline_animals,
            &records.maintenance.waste,
            &baseline_waste,
            &records.staff,
            &baseline_staff,
            &baseline_placements,
            &baseline_topology,
            &baseline_scenario,
            records.topology.as_deref().unwrap_or_default(),
            placement_records,
            &records.tanks,
            records
                .economy
                .as_ref()
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
            &records.progression,
            &records.shows,
            &records.transport,
            &records.photos,
            &records.photo_challenges,
            &entities_with_persistent_identifiers,
            &tank_persistent_identifiers,
            &mut show_stages_with_mutable_snapshot_state,
            &photo_album_persistent_identifiers,
            &mut photo_challenges_with_mutable_progress,
            progression_snapshot_application_parameters,
        )?;

    let mut planned_identifiers = std::collections::BTreeSet::new();
    for (entity, id) in &entities_with_persistent_identifiers {
        if !baseline_placements.contains(&entity)
            && !baseline_animals.contains(entity)
            && !baseline_waste.contains(entity)
            && !baseline_staff.contains(&entity)
            && !baseline_topology.contains(&entity)
            && !baseline_scenario.contains(&entity)
            && !planned_identifiers.insert(id.0)
        {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
    }
    planned_identifiers.extend(
        placement_records
            .iter()
            .map(|record| record.persistent_identifier),
    );
    planned_identifiers.extend(
        records
            .animals
            .iter()
            .map(|record| record.persistent_identifier),
    );
    planned_identifiers.extend(
        records
            .staff
            .iter()
            .map(|record| record.persistent_identifier),
    );
    planned_identifiers.extend(
        records
            .maintenance
            .waste
            .iter()
            .map(|record| record.persistent_identifier),
    );
    planned_identifiers.extend(
        records
            .topology
            .as_deref()
            .into_iter()
            .flatten()
            .map(|record| record.id().0),
    );
    let guests = runtime_continuation
        .as_ref()
        .and_then(|runtime| runtime.domain_continuation.as_ref())
        .map_or(&[][..], |domain| domain.guests.as_slice());
    super::guest_continuation_snapshot::validate_guest_continuation(guests, world_definitions)?;
    // Guests, fossil sites and recovered fossil pieces are recreated by the
    // runtime continuation, so their identities come from its records.
    let fossil_ids = runtime_continuation
        .as_ref()
        .and_then(|runtime| runtime.domain_continuation.as_ref()?.fossils.as_ref())
        .into_iter()
        .flat_map(super::fossil_continuation_snapshot::FossilContinuation::persistent_identifiers);
    for id in guests.iter().map(|guest| guest.id).chain(fossil_ids) {
        if !complete_persistent_identifiers.insert(id) || !planned_identifiers.insert(id) {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
        maximum_persistent_identifier = maximum_persistent_identifier.max(id);
    }
    for id in behavior_snapshot_records
        .iter()
        .filter_map(super::behavior_snapshot_types::BehaviorActorSnapshot::task_record)
        .filter_map(|actor| actor.staff_job.as_ref()?.persistent_identifier)
    {
        if !complete_persistent_identifiers.insert(id) {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
        maximum_persistent_identifier = maximum_persistent_identifier.max(id);
    }
    let mut saved_staff_sites = BTreeMap::new();
    if let Some(domain) = runtime_continuation
        .as_ref()
        .and_then(|record| record.domain_continuation.as_ref())
    {
        use super::staff_job_continuation_snapshot::SavedStaffJobTarget;
        for job in &domain.staff_jobs {
            if !complete_persistent_identifiers.insert(job.id) {
                return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
            }
            maximum_persistent_identifier = maximum_persistent_identifier.max(job.id);
            if job.target.is_some_and(|target| {
                !complete_persistent_identifiers.contains(&target.existing_id())
            }) || job.rejected.iter().any(|id| {
                records
                    .staff
                    .binary_search_by_key(id, |staff| staff.persistent_identifier)
                    .is_err()
            }) {
                return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
            }
            if let Some(target @ SavedStaffJobTarget::CleaningSite { id, .. }) = job.target {
                if let Some(previous) = saved_staff_sites.insert(id, target) {
                    if previous != target {
                        return Err(
                            WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference,
                        );
                    }
                } else if !complete_persistent_identifiers.insert(id) {
                    return Err(
                        WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier,
                    );
                }
                maximum_persistent_identifier = maximum_persistent_identifier.max(id);
            }
        }
    }
    for treatment in records.animals.iter().filter_map(|animal| animal.treatment) {
        if !complete_persistent_identifiers.contains(&treatment.provider) {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        if world_definitions
            .find_treatment(treatment.definition)
            .is_none()
        {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
    }
    if records
        .maintenance
        .objects
        .iter()
        .any(|record| !planned_identifiers.contains(&record.persistent_identifier))
    {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }
    // Validate references before spawning or removing any placement entities.
    let (maximum_navigation_request, maximum_behavior_execution) = super::behavior_snapshot_identity_high_water_marks::behavior_snapshot_identity_high_water_marks(behavior_snapshot_records);
    let high_water_marks =
        super::runtime_continuation_snapshot_types::RuntimeContinuationIdentityHighWaterMarks {
            maximum_persistent_identifier,
            maximum_navigation_request: guests
                .iter()
                .map(|guest| guest.navigation_request())
                .max()
                .unwrap_or_default()
                .max(maximum_navigation_request),
            maximum_behavior_execution,
            pending_service_request_sequences: &[],
        };
    if let Some(record) = &runtime_continuation {
        if let Some((id, _)) = record.entity_names.as_ref().and_then(|names| {
            names
                .iter()
                .find(|(id, _)| !complete_persistent_identifiers.contains(id))
        }) {
            bevy::log::warn!(id, "saved name owner is missing");
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        if let Some(id) = record
            .registered_entity_name_identifiers
            .as_ref()
            .and_then(|ids| {
                ids.iter()
                    .find(|id| !complete_persistent_identifiers.contains(id))
            })
        {
            bevy::log::warn!(id, "saved name registration owner is missing");
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        super::runtime_continuation_snapshot_application::validate_runtime_continuation_identity_high_water_marks(record, high_water_marks)?;
        if record
            .guest_favourite_animal_trend
            .into_iter()
            .flat_map(|trend| trend.species().into_iter().flatten())
            .any(|id| species.find(id).is_none())
        {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
    }
    if records
        .donations
        .iter()
        .any(|record| !planned_identifiers.contains(&record.identifier.0))
    {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }
    let replaced_behavior_entities = baseline_animals
        .iter()
        .chain(baseline_waste.iter())
        .chain(baseline_staff.iter().copied())
        .chain(baseline_placements.iter().copied())
        .chain(baseline_topology.iter().copied())
        .collect::<Vec<_>>();
    let imported_behavior_identifiers = planned_identifiers.iter().copied();
    if !super::behavior_snapshot_application::prepare_behavior_snapshot_before_world_replacement(
        world_root_entity,
        behavior_snapshot_records,
        &mut behavior,
        behavior_documents,
        &replaced_behavior_entities,
        imported_behavior_identifiers,
        records.topology.as_deref(),
    )? {
        return Ok(None);
    }
    let records = sections
        .as_mut()
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    super::guest_continuation_snapshot::restore_guests(
        commands,
        world_root_entity,
        guests,
        world_definitions,
        persistent_identifier_allocator,
    )?;
    records.scenario = scenario_snapshot;
    records.runtime = runtime_continuation;
    let placement_records = records.placements.as_deref().unwrap_or_default();
    super::placement_snapshot_section::apply_placement_records(
        commands,
        placement_records,
        &prefabs,
        &resolved_placement_prefabs,
        world_definitions,
        &baseline_placements,
        world_root_entity,
        persistent_identifier_allocator,
    )?;
    let retired_fence_bounds = if let Some(topology_records) = &records.topology {
        let replaced_entities = baseline_placements
            .iter()
            .copied()
            .chain(baseline_animals.iter())
            .chain(baseline_waste.iter())
            .chain(baseline_staff.iter().copied())
            .collect::<Vec<_>>();
        super::fence_topology_snapshot_section::spawn_saved_topology_nodes(
            commands,
            topology_records,
            &topology,
            world_definitions,
            persistent_identifier_allocator,
            &replaced_entities,
            &persistent_ids,
            world_root_entity,
        )?
    } else {
        None
    };
    apply_tank_snapshot_records_to_live_world(
        commands,
        std::mem::take(&mut records.tanks),
        &persistent_ids,
        &mut restored_tanks,
        world_root_entity,
    )?;
    super::animal_snapshot_application::spawn_saved_animals(
        commands,
        &records.animals,
        resolved_animals,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_animals,
    )?;
    super::maintenance_snapshot_application::apply_loose_waste_records(
        commands,
        &records.maintenance.waste,
        &resolved_waste_prefabs,
        &prefabs,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_waste,
    )?;
    super::staff_snapshot_application::apply_staff_snapshot_records(
        commands,
        &records.staff,
        &resolved_staff_prefabs,
        &prefabs,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_staff,
        world_definitions,
    )?;
    Ok(Some(SnapshotSpawns {
        retired_fence_bounds,
    }))
}

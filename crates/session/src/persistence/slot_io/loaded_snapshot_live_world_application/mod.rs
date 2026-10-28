use bevy::prelude::{Commands, Entity, Handle, MessageWriter, Name};
use openzt2_game_data::AssetId;

use openzt2_animals::animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_zoo_state::shows::show_platform_upgrade_types::RestoreShowPlatformUpgrade;

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        progression_snapshot_application::{
            apply_progression_snapshot_records_to_live_world,
            ProgressionSnapshotApplicationParameters,
        },
        progression_snapshot_section_encoding::decode_and_validate_progression_snapshot_section,
        simulation_time_snapshot_section::{
            decode_simulation_time_snapshot_record, SimulationTimeSnapshotLoadResources,
        },
        snapshot_container_encoding_and_validation::validate_world_snapshot_container_and_section_directory,
        snapshot_container_types::WorldSnapshotSectionKind,
    },
    animal_adoption_offer_inventory_snapshot_section::{
        decode_and_validate_animal_adoption_offer_inventory_snapshot_section,
        validate_animal_adoption_offer_inventory_against_loaded_authored_content,
    },
    economy_snapshot_application::apply_economy_snapshot_records_to_live_economy_resources_and_facilities,
    economy_snapshot_section_encoding::decode_and_validate_economy_snapshot_section,
    loaded_snapshot_reference_validation::validate_loaded_snapshot_references_and_imported_persistent_identifiers,
    photo_snapshot_application::apply_photo_and_challenge_progress_snapshot_records_to_live_world,
    photo_snapshot_section_encoding::decode_and_validate_photo_snapshot_section,
    show_snapshot_application::apply_show_snapshot_records_to_live_show_stages_and_schedule,
    show_snapshot_section_encoding::decode_and_validate_show_snapshot_section,
    tank_snapshot_application::apply_tank_snapshot_records_to_live_world,
    tank_snapshot_section_encoding::decode_and_validate_tank_snapshot_section,
    transport_snapshot_application::apply_transport_snapshot_records_to_live_transport_entities,
    transport_snapshot_section_encoding::decode_and_validate_transport_snapshot_section,
    world_snapshot_application_system_parameters::WorldSnapshotApplicationState,
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
};

pub(super) fn decode_validate_and_apply_snapshot_to_loaded_baseline_world(
    commands: &mut Commands,
    snapshot_container_bytes: &[u8],
    profile_identifier: AssetId,
    world_root_entity: Entity,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    world_snapshot_application_state: WorldSnapshotApplicationState,
    progression_snapshot_application_parameters: &mut ProgressionSnapshotApplicationParameters,
    simulation_time_snapshot_application_resources: &mut SimulationTimeSnapshotLoadResources,
    show_platform_upgrade_restore_requests: &mut MessageWriter<RestoreShowPlatformUpgrade>,
    retained_maintenance_prefabs: &mut Vec<
        bevy::prelude::Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    >,
    placement_hydration: &mut Option<Vec<u64>>,
    behavior_documents: &mut Vec<
        Handle<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>,
    >,
) -> Result<bool, WorldSnapshotPersistenceFailure> {
    let WorldSnapshotApplicationState {
        mut behavior,
        mut topology,
        mut scenario,
        placements,
        placement_models,
        placement_readiness,
        photo_baseline,
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
        transport_entities_with_persistent_identifiers,
        mut show_stages_with_mutable_snapshot_state,
        mut loaded_photo_images,
        mut photo_challenges_with_mutable_progress,
        tank_persistent_identifiers,
        facility_persistent_identifiers,
        mut zoo_cash,
        mut zoo_admission_price,
        mut zoo_admissions_open_state,
        mut monthly_finance_history,
        photo_album_persistent_identifiers,
    } = world_snapshot_application_state;

    let validated_snapshot_container = validate_world_snapshot_container_and_section_directory(
        snapshot_container_bytes,
        profile_identifier,
    )?;
    let behavior_snapshot_records =
        super::behavior_snapshot_section_encoding::decode_behavior_actor_snapshots(
            snapshot_container_bytes,
            validated_snapshot_container
                .section_directory_entry(WorldSnapshotSectionKind::Behavior),
        )?;

    if let Some(identifiers) = placement_hydration.as_ref() {
        let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
            return Ok(false);
        };
        let hydrated = identifiers.iter().all(|id| {
            placement_readiness
                .iter()
                .any(|(candidate, definition, transform, placed, dock)| {
                    candidate.0 == *id
                        && super::placement_snapshot_section::placement_hydration_complete(
                            definitions,
                            definition.0,
                            transform,
                            placed,
                            dock,
                        )
                })
        });
        if !hydrated {
            return Ok(false);
        }
        return super::behavior_snapshot_application::restore_behavior_actor_snapshots(
            commands,
            &behavior_snapshot_records,
            &mut behavior,
            behavior_documents,
        );
    }

    // The coordinator runs after Intent and its deferred commands. Initialization
    // must be committed before any live reference check or restored mutation.
    if !simulation_time_snapshot_application_resources.is_initialized()
        || !photo_baseline
            .is_ready_for_snapshot_application(world_root_entity, persistent_identifier_allocator)?
    {
        return Ok(false);
    }
    let Some(species) = active_species.get(&species_assets) else {
        return if active_species.catalogue_load_is_pending(&asset_server) {
            Ok(false)
        } else {
            Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)
        };
    };

    // Every payload was checked before loading this baseline world. Decode the
    // records here, then check their references against the loaded entities.
    let world_snapshot_record = decode_and_validate_world_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::World),
    )?;
    let animal_adoption_offer_inventory_snapshot_section = validated_snapshot_container
        .section_directory_entry(WorldSnapshotSectionKind::AdoptionOffers);
    let animal_adoption_offer_inventory =
        decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
            &snapshot_container_bytes[animal_adoption_offer_inventory_snapshot_section
                .payload_byte_range_within_container(snapshot_container_bytes.len())?],
            animal_adoption_offer_inventory_snapshot_section.encoded_record_count,
            world_snapshot_record.session_mode,
        )?;
    let animal_snapshot_records =
        super::animal_snapshot_section_encoding::decode_animal_snapshot_records(
            snapshot_container_bytes,
            validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Animals),
        )?;
    let scenario_snapshot = super::scenario_snapshot_section::decode(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Scenario),
    )?;
    if scenario_snapshot.is_some()
        && super::scenario_snapshot_section::scripts_are_pending(&scenario, &asset_server)
    {
        return Ok(false);
    }
    let super::maintenance_snapshot_types::MaintenanceSnapshot {
        waste: loose_waste_records,
        objects: maintained_object_records,
    } = super::maintenance_snapshot_section_encoding::decode_maintenance_snapshot(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Maintenance),
    )?;
    let staff_snapshot_records =
        super::staff_snapshot_section_encoding::decode_staff_snapshot_records(
            snapshot_container_bytes,
            validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Staff),
        )?;
    let tank_snapshot_records = decode_and_validate_tank_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Aquatic),
    )?;
    let topology_snapshot_records =
        super::fence_topology_snapshot_section::decode_fence_topology_records(
            snapshot_container_bytes,
            validated_snapshot_container
                .section_directory_entry(WorldSnapshotSectionKind::Topology),
        )?;
    let baseline_topology = if topology_snapshot_records.is_some() {
        topology
            .baseline_entities(world_root_entity)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let placement_snapshot_records = super::placement_snapshot_section::decode_placement_records(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Placement),
    )?;
    let donation_snapshot_records = super::donation_snapshot_section::decode_donation_records(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Donations),
    )?;
    let economy_snapshot_records = decode_and_validate_economy_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Economy),
    )?;
    let progression_snapshot_section =
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Progression);
    let progression_snapshot_records = decode_and_validate_progression_snapshot_section(
        &snapshot_container_bytes[progression_snapshot_section
            .payload_byte_range_within_container(snapshot_container_bytes.len())?],
        progression_snapshot_section.encoded_record_count,
    )?;
    let world_definitions = active_world_definitions
        .get(&world_definition_assets)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let placement_records = placement_snapshot_records.as_deref().unwrap_or_default();
    super::native_crate_snapshot_links::validate_native_crate_links(
        &animal_snapshot_records,
        placement_records,
    )?;
    let baseline_placements = if placement_snapshot_records.is_some() {
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
        return Ok(false);
    };
    if let Some(inventory) = animal_adoption_offer_inventory.as_ref() {
        validate_animal_adoption_offer_inventory_against_loaded_authored_content(
            inventory,
            species,
            world_definitions
                .animal_adoption_offer_configuration()
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?,
            progression_snapshot_records.fame,
        )?;
    }
    let simulation_time_snapshot_section = validated_snapshot_container
        .section_directory_entry(WorldSnapshotSectionKind::SimulationTime);
    let simulation_time_snapshot_record = decode_simulation_time_snapshot_record(
        &snapshot_container_bytes[simulation_time_snapshot_section
            .payload_byte_range_within_container(snapshot_container_bytes.len())?],
    )?;
    let show_snapshot_records = decode_and_validate_show_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Shows),
    )?;
    let transport_snapshot_records = decode_and_validate_transport_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Transport),
    )?;
    let (photo_snapshot_records, photo_challenge_progress_snapshot_records) =
        decode_and_validate_photo_snapshot_section(
            snapshot_container_bytes,
            validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Photos),
        )?;

    let Some(resolved_waste_prefabs) =
        super::maintenance_snapshot_application::resolve_loose_waste_prefabs(
            &loose_waste_records,
            world_definitions,
            &prefabs,
            &asset_server,
            retained_maintenance_prefabs,
        )?
    else {
        return Ok(false);
    };
    let Some(resolved_staff_prefabs) =
        super::staff_snapshot_application::resolve_staff_body_prefabs(
            &staff_snapshot_records,
            world_definitions,
            &prefabs,
            &asset_server,
            retained_maintenance_prefabs,
        )?
    else {
        return Ok(false);
    };
    let baseline_staff = super::staff_snapshot_application::baseline_staff_entities(
        world_definitions,
        &entities_with_definition_identifiers,
    );
    let resolved_animals = super::animal_snapshot_application::resolve_saved_animal_components(
        &animal_snapshot_records,
        species,
        world_definitions.timing().ticks_per_day,
    )?;
    validate_loaded_snapshot_references_and_imported_persistent_identifiers(
        scenario_snapshot.as_ref(),
        &animal_snapshot_records,
        &baseline_animals,
        &loose_waste_records,
        &baseline_waste,
        &staff_snapshot_records,
        &baseline_staff,
        &baseline_placements,
        &baseline_topology,
        topology_snapshot_records.as_deref().unwrap_or_default(),
        placement_records,
        &tank_snapshot_records,
        &economy_snapshot_records,
        &progression_snapshot_records,
        &show_snapshot_records,
        &transport_snapshot_records,
        &photo_snapshot_records,
        &photo_challenge_progress_snapshot_records,
        &entities_with_persistent_identifiers,
        &tank_persistent_identifiers,
        &facility_persistent_identifiers,
        &mut show_stages_with_mutable_snapshot_state,
        &photo_album_persistent_identifiers,
        &mut photo_challenges_with_mutable_progress,
        progression_snapshot_application_parameters,
    )?;

    let mut restored_identities = std::collections::BTreeMap::new();
    for (entity, id) in &entities_with_persistent_identifiers {
        if !baseline_placements.contains(&entity)
            && !baseline_animals.contains(entity)
            && !baseline_waste.contains(entity)
            && !baseline_staff.contains(&entity)
            && !baseline_topology.contains(&entity)
            && restored_identities.insert(id.0, entity).is_some()
        {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
    }
    let mut planned_identities = restored_identities.clone();
    for record in placement_records {
        planned_identities.insert(record.persistent_identifier, Entity::PLACEHOLDER);
    }
    for record in &animal_snapshot_records {
        planned_identities.insert(record.persistent_identifier, Entity::PLACEHOLDER);
    }
    for record in &staff_snapshot_records {
        planned_identities.insert(record.persistent_identifier, Entity::PLACEHOLDER);
    }
    for record in &loose_waste_records {
        planned_identities.insert(record.persistent_identifier, Entity::PLACEHOLDER);
    }
    for record in topology_snapshot_records.iter().flatten() {
        planned_identities.insert(record.id().0, Entity::PLACEHOLDER);
    }
    if maintained_object_records
        .iter()
        .any(|record| !planned_identities.contains_key(&record.persistent_identifier))
    {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }
    // Validate references before spawning or removing any placement entities.
    super::donation_snapshot_section::resolve_donation_records(
        donation_snapshot_records.clone(),
        &planned_identities,
    )?;
    let prepared_scenario = super::scenario_snapshot_section::prepare(
        scenario_snapshot,
        world_snapshot_record.scenario_definition_identifier,
        &scenario,
    )?;
    let replaced_behavior_entities = baseline_animals
        .iter()
        .chain(baseline_waste.iter())
        .chain(baseline_staff.iter().copied())
        .chain(baseline_placements.iter().copied())
        .chain(baseline_topology.iter().copied())
        .collect::<Vec<_>>();
    let imported_behavior_identifiers = planned_identities.keys().copied();
    if !super::behavior_snapshot_application::prepare_behavior_snapshot_before_world_replacement(
        &behavior_snapshot_records,
        &behavior,
        behavior_documents,
        &replaced_behavior_entities,
        imported_behavior_identifiers,
        topology_snapshot_records.as_deref(),
    )? {
        return Ok(false);
    }
    let restored_placements = super::placement_snapshot_section::apply_placement_records(
        commands,
        placement_records,
        &prefabs,
        &resolved_placement_prefabs,
        world_definitions,
        &baseline_placements,
        world_root_entity,
        persistent_identifier_allocator,
    )?;
    restored_identities.extend(
        restored_placements
            .iter()
            .map(|(id, entity)| (*id, *entity)),
    );
    if let Some(records) = topology_snapshot_records.as_deref() {
        let replaced_entities = baseline_placements
            .iter()
            .copied()
            .chain(baseline_animals.iter())
            .chain(baseline_waste.iter())
            .chain(baseline_staff.iter().copied())
            .collect::<Vec<_>>();
        restored_identities.extend(
            super::fence_topology_snapshot_section::apply_fence_topology_records(
                commands,
                records,
                &mut topology,
                world_definitions,
                persistent_identifier_allocator,
                &replaced_entities,
                world_root_entity,
            )?,
        );
    }
    apply_tank_snapshot_records_to_live_world(
        commands,
        tank_snapshot_records,
        &entities_with_persistent_identifiers,
        world_root_entity,
    )?;
    apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
        commands,
        economy_snapshot_records,
        &restored_identities,
        &mut zoo_cash,
        &mut zoo_admission_price,
        &mut zoo_admissions_open_state,
        &mut monthly_finance_history,
    )?;

    let restored_animals = super::animal_snapshot_application::apply_animal_snapshot_records(
        commands,
        &animal_snapshot_records,
        resolved_animals,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_animals,
    )?;
    super::native_crate_snapshot_links::apply_native_crate_links(
        commands,
        &animal_snapshot_records,
        &restored_animals,
        &restored_placements,
    )?;
    restored_identities.extend(restored_animals.iter().map(|(id, entity)| (*id, *entity)));
    let restored_waste = super::maintenance_snapshot_application::apply_loose_waste_records(
        commands,
        &loose_waste_records,
        &resolved_waste_prefabs,
        &prefabs,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_waste,
    )?;
    restored_identities.extend(restored_waste);
    let restored_staff = super::staff_snapshot_application::apply_staff_snapshot_records(
        commands,
        &staff_snapshot_records,
        &resolved_staff_prefabs,
        &prefabs,
        world_root_entity,
        persistent_identifier_allocator,
        &baseline_staff,
        world_definitions,
    )?;
    restored_identities.extend(restored_staff);
    super::maintenance_snapshot_application::apply_maintained_object_records(
        commands,
        &maintained_object_records,
        &restored_identities,
    );
    let resolved_donations = super::donation_snapshot_section::resolve_donation_records(
        donation_snapshot_records,
        &restored_identities,
    )?;
    super::donation_snapshot_section::apply_donation_records(commands, resolved_donations);
    commands.entity(world_root_entity).insert((
        Name::new(world_snapshot_record.display_name),
        SelectedWorldIdentity {
            requested: world_snapshot_record.requested_world_definition_identifier,
            map: world_snapshot_record.map_definition_identifier,
            start: world_snapshot_record.starting_point_definition_identifier,
            mode: world_snapshot_record.session_mode,
            profile: profile_identifier,
        },
    ));
    if let Some(inventory) = animal_adoption_offer_inventory {
        commands.entity(world_root_entity).insert(inventory);
    } else {
        commands
            .entity(world_root_entity)
            .remove::<AnimalAdoptionOfferInventory>();
    }
    apply_progression_snapshot_records_to_live_world(
        commands,
        progression_snapshot_records,
        world_root_entity,
        persistent_identifier_allocator,
        progression_snapshot_application_parameters,
    )?;
    apply_photo_and_challenge_progress_snapshot_records_to_live_world(
        commands,
        photo_snapshot_records,
        photo_challenge_progress_snapshot_records,
        world_root_entity,
        &entities_with_persistent_identifiers,
        persistent_identifier_allocator,
        &mut loaded_photo_images,
        &mut photo_challenges_with_mutable_progress,
    )?;
    apply_show_snapshot_records_to_live_show_stages_and_schedule(
        commands,
        show_snapshot_records,
        &restored_animals,
        world_root_entity,
        &entities_with_persistent_identifiers,
        &mut show_stages_with_mutable_snapshot_state,
        persistent_identifier_allocator,
        show_platform_upgrade_restore_requests,
    )?;
    apply_transport_snapshot_records_to_live_transport_entities(
        commands,
        transport_snapshot_records,
        world_root_entity,
        &entities_with_persistent_identifiers,
        &transport_entities_with_persistent_identifiers,
        persistent_identifier_allocator,
    )?;
    simulation_time_snapshot_record.apply_simulation_time_snapshot_record_to_live_resources(
        simulation_time_snapshot_application_resources,
    )?;

    super::scenario_snapshot_section::apply(
        commands,
        prepared_scenario,
        world_root_entity,
        persistent_identifier_allocator,
        &mut scenario,
    )?;
    if placement_snapshot_records.is_some() || !behavior_snapshot_records.is_empty() {
        *placement_hydration = Some(
            placement_records
                .iter()
                .map(|record| record.persistent_identifier)
                .collect(),
        );
        Ok(false)
    } else {
        Ok(true)
    }
}

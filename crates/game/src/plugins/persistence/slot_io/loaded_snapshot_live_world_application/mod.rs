use bevy::prelude::{Commands, Entity, MessageWriter, Name, Query, With};
use openzt2_game_data::AssetId;

use crate::plugins::{
    shows::show_platform_upgrade_types::RestoreShowPlatformUpgrade,
    world_spawn::{
        persistent_id_types::PersistentIdAllocator, selected_world_identity::SelectedWorldIdentity,
        world_membership_types::WorldRoot,
    },
};

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
    tank_snapshot_application::apply_tank_snapshot_records_to_existing_tank_entities,
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
    world_roots: &Query<Entity, With<WorldRoot>>,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    world_snapshot_application_state: WorldSnapshotApplicationState,
    progression_snapshot_application_parameters: &mut ProgressionSnapshotApplicationParameters,
    simulation_time_snapshot_application_resources: &mut SimulationTimeSnapshotLoadResources,
    show_platform_upgrade_restore_requests: &mut MessageWriter<RestoreShowPlatformUpgrade>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let WorldSnapshotApplicationState {
        species_assets,
        active_species,
        world_definition_assets,
        active_world_definitions,
        entities_with_persistent_identifiers,
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

    // Every payload was checked before loading this baseline world. Decode the
    // records once here, then check their references against the loaded entities.
    let validated_snapshot_container = validate_world_snapshot_container_and_section_directory(
        snapshot_container_bytes,
        profile_identifier,
    )?;
    let world_snapshot_record = decode_and_validate_world_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::World),
    )?;
    let animal_adoption_offer_inventory_snapshot_section =
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Animals);
    let animal_adoption_offer_inventory =
        decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
            &snapshot_container_bytes[animal_adoption_offer_inventory_snapshot_section
                .payload_byte_range_within_container(snapshot_container_bytes.len())?],
            animal_adoption_offer_inventory_snapshot_section.encoded_record_count,
        )?;
    let tank_snapshot_records = decode_and_validate_tank_snapshot_section(
        snapshot_container_bytes,
        validated_snapshot_container.section_directory_entry(WorldSnapshotSectionKind::Aquatic),
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
    let species = active_species
        .get(&species_assets)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let world_definitions = active_world_definitions
        .get(&world_definition_assets)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    validate_animal_adoption_offer_inventory_against_loaded_authored_content(
        &animal_adoption_offer_inventory,
        species,
        world_definitions
            .animal_adoption_offer_configuration()
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?,
        progression_snapshot_records.fame,
    )?;
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

    validate_loaded_snapshot_references_and_imported_persistent_identifiers(
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

    apply_tank_snapshot_records_to_existing_tank_entities(
        commands,
        tank_snapshot_records,
        &entities_with_persistent_identifiers,
    )?;
    apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
        commands,
        economy_snapshot_records,
        &entities_with_persistent_identifiers,
        &mut zoo_cash,
        &mut zoo_admission_price,
        &mut zoo_admissions_open_state,
        &mut monthly_finance_history,
    )?;

    let world_root_entity = world_roots
        .single()
        .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    commands.entity(world_root_entity).insert((
        Name::new(world_snapshot_record.display_name),
        SelectedWorldIdentity {
            requested: world_snapshot_record.requested_world_definition_identifier,
            map: world_snapshot_record.map_definition_identifier,
            start: world_snapshot_record.starting_point_definition_identifier,
            mode: world_snapshot_record.session_mode,
            profile: profile_identifier,
        },
        animal_adoption_offer_inventory,
    ));
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
    );

    Ok(())
}

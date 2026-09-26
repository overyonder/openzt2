use bevy::prelude::{Commands, Entity, MessageWriter, NextState, Query, Res, ResMut, With};

use crate::{
    application_lifecycle::GamePhase,
    plugins::world_spawn::{
        persistent_id_types::PersistentIdAllocator, world_load_request_acceptance::BeginWorldLoad,
        world_membership_types::WorldRoot, world_unloading::UnloadWorld,
    },
};

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    profile_types::ProfileOptions,
    progression_snapshot_application::ProgressionSnapshotApplicationParameters,
    save_slot_types::{SaveSlotId, WorldSnapshotLoadedFromSlot},
    snapshot_container_types::{ReusableWorldSnapshotByteBuffer, WorldSnapshotSectionKind},
};
use super::pending_snapshot_application_types::PendingSnapshotApplication;
use super::{
    loaded_snapshot_live_world_application::decode_validate_and_apply_snapshot_to_loaded_baseline_world,
    persistence_failure_message_publication::publish_persistence_failure_for_slot_operation,
    supported_snapshot_section_validation::validate_supported_snapshot_container_and_every_section_payload,
    world_snapshot_application_system_parameters::{
        WorldSnapshotApplicationMessages, WorldSnapshotApplicationState,
    },
    world_snapshot_section_encoding::decode_and_validate_world_snapshot_section,
};

pub(in super::super) fn coordinate_pending_snapshot_validation_baseline_world_loading_and_application(
    mut commands: Commands,
    mut pending_snapshot_application: Query<(Entity, &mut PendingSnapshotApplication)>,
    profile_options: Res<ProfileOptions>,
    world_roots: Query<Entity, With<WorldRoot>>,
    mut next_game_phase: ResMut<NextState<GamePhase>>,
    mut persistent_identifier_allocator: ResMut<PersistentIdAllocator>,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    world_snapshot_application_messages: WorldSnapshotApplicationMessages,
    live_world_snapshot_application_state: WorldSnapshotApplicationState,
    mut progression_snapshot_application_parameters: ProgressionSnapshotApplicationParameters,
    mut simulation_time_snapshot_application_resources: super::super::simulation_time_snapshot_section::SimulationTimeSnapshotLoadResources,
) {
    let WorldSnapshotApplicationMessages {
        mut begin_world_load_requests,
        mut world_unload_requests,
        mut completed_world_loads,
        mut failed_world_loads,
        mut completed_snapshot_loads,
        mut persistence_failures,
        mut show_platform_upgrade_restore_requests,
    } = world_snapshot_application_messages;
    let Ok((pending_snapshot_application_entity, mut pending_snapshot_application)) =
        pending_snapshot_application.single_mut()
    else {
        return;
    };
    if pending_snapshot_application.baseline_world_load_requested {
        if !snapshot_scratch_storage.contains_validated_snapshot_container {
            discard_pending_snapshot_application_after_load_failure(
                &mut commands,
                pending_snapshot_application_entity,
                pending_snapshot_application.save_slot_identifier,
                WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
                &mut snapshot_scratch_storage,
                &mut persistence_failures,
            );
            return;
        }
        if !pending_snapshot_application.baseline_world_load_completed {
            if completed_world_loads.read().next().is_some() {
                pending_snapshot_application.baseline_world_load_completed = true;
            } else if failed_world_loads.read().next().is_some() {
                discard_pending_snapshot_application_after_load_failure(
                    &mut commands,
                    pending_snapshot_application_entity,
                    pending_snapshot_application.save_slot_identifier,
                    WorldSnapshotPersistenceFailure::UnknownAssetIdentifier,
                    &mut snapshot_scratch_storage,
                    &mut persistence_failures,
                );
            }
            return;
        }
        let snapshot_application_result =
            decode_validate_and_apply_snapshot_to_loaded_baseline_world(
                &mut commands,
                &snapshot_scratch_storage.snapshot_container_bytes,
                profile_options.profile_identifier,
                &world_roots,
                &mut persistent_identifier_allocator,
                live_world_snapshot_application_state,
                &mut progression_snapshot_application_parameters,
                &mut simulation_time_snapshot_application_resources,
                &mut show_platform_upgrade_restore_requests,
            );
        match snapshot_application_result {
            Ok(()) => {
                commands
                    .entity(pending_snapshot_application_entity)
                    .despawn();
                completed_snapshot_loads.write(WorldSnapshotLoadedFromSlot {
                    save_slot_identifier: pending_snapshot_application.save_slot_identifier,
                });
                snapshot_scratch_storage.clear_logical_contents();
            }
            Err(persistence_failure) => {
                discard_pending_snapshot_application_after_load_failure(
                    &mut commands,
                    pending_snapshot_application_entity,
                    pending_snapshot_application.save_slot_identifier,
                    persistence_failure,
                    &mut snapshot_scratch_storage,
                    &mut persistence_failures,
                );
            }
        }
        return;
    }
    if let (Some(scenario), Some(mode)) = (
        pending_snapshot_application.scenario_definition_identifier,
        pending_snapshot_application.session_mode,
    ) {
        if !world_roots.is_empty() {
            return;
        }
        begin_world_load_requests.write(BeginWorldLoad {
            scenario,
            mode,
            profile: profile_options.profile_identifier,
            starting_cash_cents: None,
        });
        pending_snapshot_application.baseline_world_load_requested = true;
        return;
    }
    let snapshot = match validate_supported_snapshot_container_and_every_section_payload(
        &snapshot_scratch_storage.snapshot_container_bytes,
        profile_options.profile_identifier,
    ) {
        Ok(snapshot) => snapshot,
        Err(reason) => {
            discard_pending_snapshot_application_after_load_failure(
                &mut commands,
                pending_snapshot_application_entity,
                pending_snapshot_application.save_slot_identifier,
                reason,
                &mut snapshot_scratch_storage,
                &mut persistence_failures,
            );
            return;
        }
    };
    snapshot_scratch_storage.contains_validated_snapshot_container = true;
    let world = match decode_and_validate_world_snapshot_section(
        &snapshot_scratch_storage.snapshot_container_bytes,
        snapshot.section_directory_entry(WorldSnapshotSectionKind::World),
    ) {
        Ok(world) => world,
        Err(reason) => {
            discard_pending_snapshot_application_after_load_failure(
                &mut commands,
                pending_snapshot_application_entity,
                pending_snapshot_application.save_slot_identifier,
                reason,
                &mut snapshot_scratch_storage,
                &mut persistence_failures,
            );
            return;
        }
    };

    for entity in &world_roots {
        world_unload_requests.write(UnloadWorld(entity));
    }
    pending_snapshot_application.scenario_definition_identifier =
        Some(world.scenario_definition_identifier);
    pending_snapshot_application.session_mode = Some(world.session_mode);
    next_game_phase.set(GamePhase::Loading);
}

fn discard_pending_snapshot_application_after_load_failure(
    commands: &mut Commands,
    pending_snapshot_application_entity: Entity,
    save_slot_identifier: SaveSlotId,
    persistence_failure: WorldSnapshotPersistenceFailure,
    snapshot_scratch_storage: &mut ReusableWorldSnapshotByteBuffer,
    persistence_failure_messages: &mut MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    commands
        .entity(pending_snapshot_application_entity)
        .despawn();
    publish_persistence_failure_for_slot_operation(
        persistence_failure_messages,
        WorldSnapshotPersistenceOperation::LoadFromSlot,
        save_slot_identifier,
        persistence_failure,
    );
    snapshot_scratch_storage.clear_logical_contents();
}

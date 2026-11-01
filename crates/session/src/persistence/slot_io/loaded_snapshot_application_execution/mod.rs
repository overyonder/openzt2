use bevy::prelude::{Commands, Entity, Local, MessageWriter, NextState, Query, Res, ResMut, With};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;
use openzt2_interface_state::persistence::saved_game_source_types::{
    OriginalSavedGameLoadFailed, OriginalSavedGameLoaded, SavedGameSource,
};

use super::super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    progression_snapshot_application::ProgressionSnapshotApplicationParameters,
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
    replacement_worlds: Query<(Entity, &WorldLoadOperationOwner), With<WorldRoot>>,
    mut next_game_phase: ResMut<NextState<GamePhase>>,
    mut persistent_identifier_allocator: Option<ResMut<PersistentIdAllocator>>,
    mut snapshot_scratch_storage: ResMut<ReusableWorldSnapshotByteBuffer>,
    world_snapshot_application_messages: WorldSnapshotApplicationMessages,
    live_world_snapshot_application_state: WorldSnapshotApplicationState,
    mut progression_snapshot_application_parameters: ProgressionSnapshotApplicationParameters,
    mut simulation_time_snapshot_application_resources: super::super::simulation_time_snapshot_section::SimulationTimeSnapshotLoadResources,
    mut reported_load_state: Local<Option<String>>,
    mut terrain_application: super::terrain_snapshot_application_parameters::TerrainSnapshotApplicationParameters,
    mut saved_world_preparation: super::saved_world_snapshot_preparation::SavedWorldSnapshotPreparation,
) {
    let WorldSnapshotApplicationMessages {
        mut begin_world_load_requests,
        mut world_unload_requests,
        mut completed_world_loads,
        mut failed_world_loads,
        mut completed_snapshot_loads,
        mut original_snapshot_loads,
        mut original_snapshot_failures,
        mut persistence_failures,
        mut show_platform_upgrade_restore_requests,
    } = world_snapshot_application_messages;
    let Ok((pending_snapshot_application_entity, mut pending_snapshot_application)) =
        pending_snapshot_application.single_mut()
    else {
        *reported_load_state = None;
        return;
    };
    let photo_readiness = pending_snapshot_application
        .baseline_world_root
        .and_then(|root| {
            persistent_identifier_allocator.as_ref().map(|allocator| {
                live_world_snapshot_application_state
                    .photo_baseline
                    .is_ready_for_snapshot_application(root, allocator)
            })
        });
    let load_state = format!(
        "{pending_snapshot_application:?}; clock_ready={}; photo_readiness={photo_readiness:?}; species_ready={}",
        simulation_time_snapshot_application_resources.is_initialized(),
        live_world_snapshot_application_state.active_species
            .get(&live_world_snapshot_application_state.species_assets).is_some(),
    );
    if reported_load_state.as_ref() != Some(&load_state) {
        bevy::log::info!(state = %load_state, "snapshot application state changed");
        *reported_load_state = Some(load_state);
    }
    if pending_snapshot_application.baseline_world_load_requested {
        if pending_snapshot_application.baseline_world_root.is_none() {
            pending_snapshot_application.baseline_world_root =
                replacement_worlds.iter().find_map(|(root, owner)| {
                    (owner.0 == pending_snapshot_application_entity).then_some(root)
                });
        }
        if !snapshot_scratch_storage.contains_validated_snapshot_container {
            if let Some(root) = pending_snapshot_application.baseline_world_root {
                world_unload_requests.write(UnloadWorld(root));
            }
            next_game_phase.set(GamePhase::MapSelection);
            discard_pending_snapshot_application_after_load_failure(
                &mut commands,
                pending_snapshot_application_entity,
                pending_snapshot_application.source,
                WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
                &mut snapshot_scratch_storage,
                &mut persistence_failures,
                &mut original_snapshot_failures,
            );
            return;
        }
        if !pending_snapshot_application.baseline_world_load_completed {
            let baseline_failed = failed_world_loads.read().any(|result| {
                result.operation == Some(pending_snapshot_application_entity)
                    && Some(result.scenario)
                        == pending_snapshot_application.scenario_definition_identifier
                    && pending_snapshot_application
                        .baseline_world_root
                        .is_none_or(|root| result.root == Some(root))
            });
            if baseline_failed {
                next_game_phase.set(GamePhase::MapSelection);
                discard_pending_snapshot_application_after_load_failure(
                    &mut commands,
                    pending_snapshot_application_entity,
                    pending_snapshot_application.source,
                    WorldSnapshotPersistenceFailure::UnknownAssetIdentifier,
                    &mut snapshot_scratch_storage,
                    &mut persistence_failures,
                    &mut original_snapshot_failures,
                );
            } else {
                for result in completed_world_loads.read() {
                    if result.operation == Some(pending_snapshot_application_entity)
                        && Some(result.scenario)
                            == pending_snapshot_application.scenario_definition_identifier
                        && Some(result.root) == pending_snapshot_application.baseline_world_root
                    {
                        pending_snapshot_application.baseline_world_load_completed = true;
                    }
                }
            }
            return;
        }
        let Some(root) = pending_snapshot_application.baseline_world_root else {
            return;
        };
        if !pending_snapshot_application.terrain_restored {
            let terrain_result =
                if let Some(terrain) = &pending_snapshot_application.terrain_snapshot {
                    terrain_application
                        .apply(
                            terrain,
                            root,
                            pending_snapshot_application_entity,
                            &mut commands,
                        )
                        .map(|()| true)
                } else {
                    Ok(false)
                };
            match terrain_result {
                Ok(changed) => {
                    pending_snapshot_application.terrain_restored = true;
                    pending_snapshot_application.terrain_snapshot = None;
                    if changed {
                        return;
                    }
                }
                Err(WorldSnapshotPersistenceFailure::SnapshotTransactionInFlight) => return,
                Err(reason) => {
                    world_unload_requests.write(UnloadWorld(root));
                    next_game_phase.set(GamePhase::MapSelection);
                    discard_pending_snapshot_application_after_load_failure(
                        &mut commands,
                        pending_snapshot_application_entity,
                        pending_snapshot_application.source,
                        reason,
                        &mut snapshot_scratch_storage,
                        &mut persistence_failures,
                        &mut original_snapshot_failures,
                    );
                    return;
                }
            }
        }
        let snapshot_application_result = match persistent_identifier_allocator.as_mut() {
            Some(persistent_identifier_allocator)
                if world_roots.get(root).is_ok()
                    && persistent_identifier_allocator.owns_world(root) =>
            {
                let pending = &mut *pending_snapshot_application;
                decode_validate_and_apply_snapshot_to_loaded_baseline_world(
                    &mut commands,
                    &snapshot_scratch_storage.snapshot_container_bytes,
                    profile_options.profile_identifier,
                    pending.source,
                    root,
                    persistent_identifier_allocator,
                    live_world_snapshot_application_state,
                    &mut progression_snapshot_application_parameters,
                    &mut simulation_time_snapshot_application_resources,
                    &mut show_platform_upgrade_restore_requests,
                    &mut pending.maintenance_prefabs,
                    &mut pending.placement_hydration,
                    &mut pending.behavior_documents,
                )
            }
            _ => Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference),
        };
        match snapshot_application_result {
            Ok(false) => {}
            Ok(true) => {
                bevy::log::info!(?root, "snapshot application completed; entering game");
                next_game_phase.set(GamePhase::InGame);
                commands
                    .entity(pending_snapshot_application_entity)
                    .despawn();
                commands.entity(root).remove::<WorldLoadOperationOwner>();
                match pending_snapshot_application.source {
                    SavedGameSource::Native(save_slot_identifier) => {
                        completed_snapshot_loads.write(WorldSnapshotLoadedFromSlot {
                            save_slot_identifier,
                        });
                    }
                    SavedGameSource::Original(source_identifier) => {
                        original_snapshot_loads
                            .write(OriginalSavedGameLoaded { source_identifier });
                    }
                }
                snapshot_scratch_storage.clear_logical_contents();
            }
            Err(persistence_failure) => {
                bevy::log::warn!(
                    ?root,
                    ?persistence_failure,
                    "snapshot application failed; returning to map selection"
                );
                world_unload_requests.write(UnloadWorld(root));
                next_game_phase.set(GamePhase::MapSelection);
                discard_pending_snapshot_application_after_load_failure(
                    &mut commands,
                    pending_snapshot_application_entity,
                    pending_snapshot_application.source,
                    persistence_failure,
                    &mut snapshot_scratch_storage,
                    &mut persistence_failures,
                    &mut original_snapshot_failures,
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
        // A later operation must not consume an earlier load's retained messages.
        completed_world_loads.clear();
        failed_world_loads.clear();
        begin_world_load_requests.write(BeginWorldLoad {
            saved_world_document: pending_snapshot_application
                .saved_world_document
                .as_ref()
                .map(|handle| handle.id()),
            operation: Some(pending_snapshot_application_entity),
            scenario,
            mode,
            profile: profile_options.profile_identifier,
            starting_cash_cents: None,
        });
        pending_snapshot_application.baseline_world_load_requested = true;
        return;
    }
    if pending_snapshot_application.world_snapshot.is_none() {
        let decoded = validate_supported_snapshot_container_and_every_section_payload(
            &snapshot_scratch_storage.snapshot_container_bytes,
            profile_options.profile_identifier,
        )
        .and_then(|snapshot| {
            let world = decode_and_validate_world_snapshot_section(
                &snapshot_scratch_storage.snapshot_container_bytes,
                snapshot.section_directory_entry(WorldSnapshotSectionKind::World),
            )?;
            let terrain =
                super::terrain_snapshot_container_section::decode_terrain_snapshot_section(
                    &snapshot_scratch_storage.snapshot_container_bytes,
                    snapshot.section_directory_entry(WorldSnapshotSectionKind::Terrain),
                )?;
            Ok((world, terrain))
        });
        match decoded {
            Ok((world, terrain)) => {
                pending_snapshot_application.world_snapshot = Some(world);
                pending_snapshot_application.terrain_snapshot = terrain;
                snapshot_scratch_storage.contains_validated_snapshot_container = true;
            }
            Err(reason) => {
                discard_pending_snapshot_application_after_load_failure(
                    &mut commands,
                    pending_snapshot_application_entity,
                    pending_snapshot_application.source,
                    reason,
                    &mut snapshot_scratch_storage,
                    &mut persistence_failures,
                    &mut original_snapshot_failures,
                );
                return;
            }
        }
    }
    let preparation = {
        let PendingSnapshotApplication {
            world_snapshot,
            terrain_snapshot,
            saved_world_document,
            ..
        } = &mut *pending_snapshot_application;
        let Some(world) = world_snapshot.as_ref() else {
            return;
        };
        saved_world_preparation.prepare(world, terrain_snapshot.as_ref(), saved_world_document)
    };
    match preparation {
        Ok(false) => return,
        Ok(true) => {}
        Err(reason) => {
            discard_pending_snapshot_application_after_load_failure(
                &mut commands,
                pending_snapshot_application_entity,
                pending_snapshot_application.source,
                reason,
                &mut snapshot_scratch_storage,
                &mut persistence_failures,
                &mut original_snapshot_failures,
            );
            return;
        }
    }
    let Some(world) = pending_snapshot_application.world_snapshot.take() else {
        return;
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
    source: SavedGameSource,
    persistence_failure: WorldSnapshotPersistenceFailure,
    snapshot_scratch_storage: &mut ReusableWorldSnapshotByteBuffer,
    persistence_failure_messages: &mut MessageWriter<WorldSnapshotPersistenceFailed>,
    original_failures: &mut MessageWriter<OriginalSavedGameLoadFailed>,
) {
    commands
        .entity(pending_snapshot_application_entity)
        .despawn();
    match source {
        SavedGameSource::Native(save_slot_identifier) => {
            publish_persistence_failure_for_slot_operation(
                persistence_failure_messages,
                WorldSnapshotPersistenceOperation::LoadFromSlot,
                save_slot_identifier,
                persistence_failure,
            )
        }
        SavedGameSource::Original(source_identifier) => {
            original_failures.write(OriginalSavedGameLoadFailed {
                source_identifier,
                reason: format!("{persistence_failure:?}"),
            });
        }
    }
    snapshot_scratch_storage.clear_logical_contents();
}

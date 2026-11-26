mod baseline;
mod continuation;
mod domains;
mod sections;

use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState, UntypedAssetLoadFailedEvent},
    ecs::{
        schedule::ScheduleConfigs,
        system::{ScheduleSystem, SystemParam},
    },
    prelude::*,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    lua_script::lua_script_asset_loading::LuaScriptAsset,
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_interface_state::persistence::{
    profile_types::ProfileOptions,
    save_slot_types::WorldSnapshotLoadedFromSlot,
    saved_game_source_types::{
        OriginalSavedGameLoadFailed, OriginalSavedGameLoaded, SavedGameSource,
    },
};
use openzt2_interface_state::photos::photo_challenge_types::{
    PhotoChallengeCatalogueIndex, PhotoChallengeProgress,
};
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex,
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_hydration_completion::WorldLoadFinished,
    world_load_failure::WorldLoadFailed,
    world_load_request_acceptance::{BeginWorldLoad, WorldLoadOperationOwner},
    world_membership_types::{DefinitionId, WorldRoot},
    world_unloading::UnloadWorld,
};
use openzt2_zoo_state::{
    locomotion::locomotion_types::LocomotionCapacity,
    progression::research_types::{
        ResearchProject, ResearchProjectPersistentId, WorldResearchItems,
    },
    topology::topology_graph_types::{TopologyGrid, TopologyNode},
};

use super::{
    super::{
        persistence_failure_types::{
            WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure as Failure,
            WorldSnapshotPersistenceOperation,
        },
        progression_snapshot_application::{
            restore_saved_research_projects, ProgressionSnapshotApplicationParameters,
            SavedResearchProjects,
        },
        simulation_time_snapshot_section::SimulationTimeSnapshotRestore,
        snapshot_container_types::ReusableWorldSnapshotByteBuffer,
    },
    behavior_snapshot_application::{
        parameters::BehaviorSnapshotApplication, restore_behavior_actor_snapshots,
    },
    donation_snapshot_section::{restore_donation_totals, RestoredDonationComponents},
    economy_snapshot_application::{
        apply_economy_snapshot_records, RestoredFacilityEconomy, RestoredZooEconomy,
    },
    fence_topology_snapshot_section::restore_snapshot_fence_edges,
    loaded_snapshot_live_world_application::restore_snapshot_sections,
    maintenance_snapshot_application::{
        apply_maintained_object_records, RestoredMaintenanceComponents,
    },
    pending_snapshot_application_types::{
        AttachSavedWorldState, AttachSnapshotDonations, AttachSnapshotEconomy,
        AttachSnapshotMaintenance, AwaitingSavedWorldDocument, AwaitingSnapshotDomainBaseline,
        AwaitingSnapshotTerrainNavigation, AwaitingSnapshotWorldLoad, AwaitingWorldRetirement,
        PendingSnapshotApplication, PrepareSnapshotBaseline, RestoreRuntimeContinuation,
        RestoreSavedFenceEdges, RestoreSnapshotSections, RestoreSnapshotTerrain,
        RestoreTransportIdentities, RestoreTransportState, ResumeSnapshotActors, SnapshotBytes,
        SnapshotRestoreFailed, SnapshotRestored, SnapshotSectionRestore, SnapshotTerrainAsset,
    },
    persistence_failure_message_publication::publish_persistence_failure_for_slot_operation,
    photo_snapshot_application::restore_snapshot_photo_challenge_completion,
    photo_snapshot_baseline_readiness::PhotoSnapshotBaselineReadiness,
    runtime_continuation_snapshot_application::{
        restore_entity_name_registration, restore_saved_entity_names, restore_world_audio_preset,
        RuntimeContinuationApplication, SavedEntityNames, SavedNameRegistration,
    },
    saved_world_snapshot_preparation::SavedWorldSnapshotPreparation,
    supported_snapshot_section_validation::{decode_snapshot_for_world_restore, DecodedSnapshot},
    terrain_snapshot_application_parameters::TerrainSnapshotApplicationParameters,
    transport_snapshot_application::{
        restore_transport_sections, spawn_missing_transport_identities,
        RestoredTransportComponents, SavedTransportEntities,
    },
    world_snapshot_application_system_parameters::WorldSnapshotApplicationState,
};

fn finish_snapshot_restore(
    mut commands: Commands,
    restored: Query<(Entity, &PendingSnapshotApplication, &SnapshotRestored)>,
    failed: Query<(Entity, &PendingSnapshotApplication, &SnapshotRestoreFailed)>,
    mut phase: ResMut<NextState<GamePhase>>,
    mut unload: MessageWriter<UnloadWorld>,
    mut loaded: MessageWriter<WorldSnapshotLoadedFromSlot>,
    mut loaded_original: MessageWriter<OriginalSavedGameLoaded>,
    mut failures: MessageWriter<WorldSnapshotPersistenceFailed>,
    mut original_failures: MessageWriter<OriginalSavedGameLoadFailed>,
) {
    for (operation, pending, failure) in &failed {
        let reason = match *failure {
            SnapshotRestoreFailed::BeforeReplacement(reason) => reason,
            SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload,
            } => {
                if let Some(root) = root_to_unload {
                    unload.write(UnloadWorld(root));
                }
                phase.set(GamePhase::MapSelection);
                reason
            }
        };
        match pending.source {
            SavedGameSource::Native(slot) => {
                publish_persistence_failure_for_slot_operation(
                    &mut failures,
                    WorldSnapshotPersistenceOperation::LoadFromSlot,
                    slot,
                    reason,
                );
            }
            SavedGameSource::Original(source_identifier) => {
                original_failures.write(OriginalSavedGameLoadFailed {
                    source_identifier,
                    reason: format!("{reason:?}"),
                });
            }
        }
        commands.entity(operation).despawn();
    }
    for (operation, pending, restored) in &restored {
        let root = restored.0;
        info!(?root, "snapshot application completed; entering game");
        phase.set(GamePhase::InGame);
        commands.entity(root).remove::<WorldLoadOperationOwner>();
        match pending.source {
            SavedGameSource::Native(save_slot_identifier) => {
                loaded.write(WorldSnapshotLoadedFromSlot {
                    save_slot_identifier,
                });
            }
            SavedGameSource::Original(source_identifier) => {
                loaded_original.write(OriginalSavedGameLoaded { source_identifier });
            }
        }
        commands.entity(operation).despawn();
    }
}

pub(in crate::persistence) fn restore_pending_snapshots() -> ScheduleConfigs<ScheduleSystem> {
    (
        (
            baseline::decode_snapshot_operation.run_if(any_with_component::<SnapshotBytes>),
            baseline::prepare_snapshot_baseline
                .run_if(any_with_component::<PrepareSnapshotBaseline>),
            baseline::retire_world_once_saved_assets_are_ready,
        )
            .chain(),
        baseline::begin_snapshot_baseline_load
            .run_if(any_with_component::<AwaitingWorldRetirement>)
            .run_if(in_state(GamePhase::Loading))
            .after(baseline::retire_world_once_saved_assets_are_ready),
        (
            baseline::receive_snapshot_baseline_result,
            baseline::restore_snapshot_terrain.run_if(any_with_component::<RestoreSnapshotTerrain>),
            baseline::accept_restored_terrain_navigation
                .run_if(any_with_component::<AwaitingSnapshotTerrainNavigation>),
            baseline::accept_snapshot_domain_baseline
                .run_if(any_with_component::<AwaitingSnapshotDomainBaseline>),
            sections::apply_snapshot_sections
                .run_if(sections::snapshot_section_dependencies_changed),
            (
                sections::prepare_transport_identities
                    .run_if(any_with_component::<RestoreTransportIdentities>),
                sections::apply_saved_fence_edges
                    .run_if(any_with_component::<AttachSavedWorldState>),
            ),
            (
                sections::attach_saved_economy,
                sections::attach_saved_maintenance,
                sections::attach_saved_donations,
            )
                .run_if(any_with_component::<AttachSavedWorldState>),
            sections::distribute_saved_snapshot_sections
                .run_if(any_with_component::<AttachSavedWorldState>),
            (
                domains::restore_snapshot_clock,
                domains::restore_world_metadata,
                domains::restore_snapshot_animal_links,
                domains::restore_snapshot_adoption,
                domains::restore_snapshot_progression,
                domains::restore_snapshot_shows,
                domains::restore_snapshot_images,
            )
                .run_if(any_with_component::<RestoreRuntimeContinuation>),
            (
                sections::restore_transport_state
                    .run_if(any_with_component::<RestoreTransportState>),
                sections::restore_research_projects
                    .run_if(any_with_component::<SavedResearchProjects>),
            ),
            domains::restore_snapshot_scenario
                .run_if(any_with_component::<RestoreRuntimeContinuation>),
            (
                continuation::restore_runtime_continuation,
                continuation::restore_fossil_continuation
                    .after(continuation::restore_runtime_continuation)
                    .before(continuation::restore_snapshot_names)
                    .before(continuation::restore_snapshot_name_registration),
                continuation::restore_snapshot_names,
                continuation::restore_snapshot_name_registration,
                continuation::restore_snapshot_audio,
                continuation::restore_photo_challenge_completion,
            )
                .run_if(any_with_component::<RestoreRuntimeContinuation>),
            continuation::runtime_continuation_is_restored
                .run_if(any_with_component::<RestoreRuntimeContinuation>),
        )
            .chain(),
        (
            continuation::resume_snapshot_actors
                .run_if(any_with_component::<ResumeSnapshotActors>),
            super::behavior_snapshot_application::state_application::bind_restored_behavior_instruction_state
                .run_if(any_with_component::<super::behavior_snapshot_application::state_application::RestoredBehaviorInstructionState>),
        )
            .chain()
            .after(continuation::runtime_continuation_is_restored),
        super::animal_snapshot_application::restore_animal_treatments
            .after(continuation::resume_snapshot_actors)
            .run_if(any_with_component::<super::animal_snapshot_application::PendingSavedAnimalTreatment>),
        finish_snapshot_restore
            .after(super::animal_snapshot_application::restore_animal_treatments)
            .after(baseline::retire_world_once_saved_assets_are_ready)
            .after(super::behavior_snapshot_application::state_application::bind_restored_behavior_instruction_state),
    )
        .into_configs()
}

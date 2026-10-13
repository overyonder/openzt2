use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::FossilSetAssembly;
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::FossilRecoverySite;

use super::immersive_mode_entry_and_policy_operations::commit_authored_interaction_tool_immersive_mode_entry;
use super::immersive_mode_entry_and_policy_operations::enter_immersive_mode_for_valid_subject_with_authored_tool;
use super::immersive_mode_entry_and_policy_operations::find_pending_entry_for_immersive_mode;
use super::immersive_mode_entry_and_policy_operations::record_pending_immersive_mode_entry_failure;
use super::immersive_mode_entry_and_policy_operations::require_subject_from_pending_immersive_mode_entry;
use super::immersive_mode_message_types::ImmersiveModeEntered;
use super::immersive_mode_message_types::ModeEntryFailure;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::FossilAssemblyControl;
use crate::immersive_modes::immersive_control_execution_types::FossilSearchControl;
use crate::immersive_modes::immersive_control_execution_types::PendingImmersiveEntry;

pub(super) fn enter_fossil_search_for_pending_unexhausted_recovery_site(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    fossil_recovery_sites: Query<&FossilRecoverySite>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller_entity, pending_entry)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::FossilSearch, &pending_entries)
    else {
        return;
    };
    let Some(fossil_recovery_site_entity) = require_subject_from_pending_immersive_mode_entry(
        controller_entity,
        pending_entry,
        &mut commands,
    ) else {
        return;
    };
    let Ok(fossil_recovery_site) = fossil_recovery_sites.get(fossil_recovery_site_entity) else {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::WrongSubject,
            &mut commands,
        );
        return;
    };
    if fossil_recovery_site.all_pieces_collected {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::WrongSubject,
            &mut commands,
        );
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    let _ = commit_authored_interaction_tool_immersive_mode_entry(
        ImmersiveMode::FossilSearch,
        controller_entity,
        fossil_recovery_site_entity,
        pending_entry,
        world_definitions,
        &zoo_cameras,
        FossilSearchControl,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    );
}

pub(super) fn enter_fossil_assembly_for_pending_fossil_set_assembly(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    fossil_set_assemblies: Query<&FossilSetAssembly>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    enter_immersive_mode_for_valid_subject_with_authored_tool(
        ImmersiveMode::FossilAssembly,
        &pending_entries,
        &fossil_set_assemblies,
        active_world_definitions.get(&world_definition_assets),
        &zoo_cameras,
        FossilAssemblyControl,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    );
}

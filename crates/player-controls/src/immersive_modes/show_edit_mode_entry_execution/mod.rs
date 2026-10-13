use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::entity_selection_types::InfoPanel;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowPerformancePlan;
use openzt2_zoo_state::shows::show_stage_types::ShowStage;

use super::immersive_mode_entry_and_policy_operations::commit_authored_interaction_tool_immersive_mode_entry;
use super::immersive_mode_entry_and_policy_operations::find_pending_entry_for_immersive_mode;
use super::immersive_mode_entry_and_policy_operations::record_pending_immersive_mode_entry_failure;
use super::immersive_mode_entry_and_policy_operations::require_subject_from_pending_immersive_mode_entry;
use super::immersive_mode_message_types::ImmersiveModeEntered;
use super::immersive_mode_message_types::ModeEntryFailure;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::PendingImmersiveEntry;
use crate::immersive_modes::immersive_control_execution_types::ShowEditControl;

pub(super) fn enter_show_edit_for_pending_stage_with_information_panel(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    show_stages: Query<(&ShowStage, &ScheduledShowPerformancePlan)>,
    information_panels: Query<&InfoPanel>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller_entity, pending_entry)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::ShowEdit, &pending_entries)
    else {
        return;
    };
    let Some(show_stage_entity) = require_subject_from_pending_immersive_mode_entry(
        controller_entity,
        pending_entry,
        &mut commands,
    ) else {
        return;
    };
    if show_stages.get(show_stage_entity).is_err()
        || !information_panels
            .iter()
            .any(|information_panel| information_panel.subject == show_stage_entity)
    {
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
        ImmersiveMode::ShowEdit,
        controller_entity,
        show_stage_entity,
        pending_entry,
        world_definitions,
        &zoo_cameras,
        ShowEditControl,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    );
}

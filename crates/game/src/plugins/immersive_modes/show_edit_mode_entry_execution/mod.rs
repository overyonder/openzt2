use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_control_message_types::SetCameraMode;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::information::entity_selection_types::InfoPanel;
use crate::plugins::shows::show_schedule_types::ScheduledShowPerformancePlan;
use crate::plugins::shows::show_stage_types::ShowStage;

use super::{
    immersive_mode_control_types::ShowEditControl,
    immersive_mode_entry_and_policy_operations::{
        commit_authored_interaction_tool_immersive_mode_entry,
        find_pending_entry_for_immersive_mode, record_pending_immersive_mode_entry_failure,
        require_subject_from_pending_immersive_mode_entry,
    },
    immersive_mode_message_types::{ImmersiveModeEntered, ModeEntryFailure},
    immersive_mode_state_types::{ImmersiveMode, PendingImmersiveEntry},
};

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

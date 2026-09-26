use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_control_message_types::SetCameraMode;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::photos::photo_capture_types::PhotoReadback;

use super::{
    immersive_mode_control_types::PhotoControl,
    immersive_mode_entry_and_policy_operations::{
        commit_authored_interaction_tool_immersive_mode_entry,
        find_pending_entry_for_immersive_mode, record_pending_immersive_mode_entry_failure,
    },
    immersive_mode_message_types::{ImmersiveModeEntered, ModeEntryFailure},
    immersive_mode_state_types::{ImmersiveMode, PendingImmersiveEntry},
};

pub(super) fn enter_photo_mode_when_readback_is_available_and_idle(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    photo_readback: Option<Res<PhotoReadback>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller_entity, pending_entry)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::Photo, &pending_entries)
    else {
        return;
    };
    let Some(photo_readback) = photo_readback else {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    if photo_readback.busy {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::RuleDenied,
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
    let subject_entity = pending_entry.subject.unwrap_or(controller_entity);
    let _ = commit_authored_interaction_tool_immersive_mode_entry(
        ImmersiveMode::Photo,
        controller_entity,
        subject_entity,
        pending_entry,
        world_definitions,
        &zoo_cameras,
        PhotoControl,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    );
}

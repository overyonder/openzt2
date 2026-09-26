use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::Dead;
use crate::plugins::camera::camera_control_message_types::SetCameraMode;
use crate::plugins::camera::camera_runtime_state_types::CameraMode;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::transport_tours::transport_vehicle_types::TransportVehicle;

use super::{
    immersive_mode_control_types::{FirstPersonControl, GuestViewControl, GuestViewTileCount},
    immersive_mode_entry_and_policy_operations::{
        attach_authored_immersive_mode_consumer_policies,
        calculate_initial_first_person_view_angles, commit_validated_immersive_mode_entry,
        find_pending_entry_for_immersive_mode, record_pending_immersive_mode_entry_failure,
        require_subject_from_pending_immersive_mode_entry,
        resolve_required_authored_interaction_policy,
    },
    immersive_mode_message_types::{ImmersiveModeEntered, ModeEntryFailure},
    immersive_mode_state_types::{ImmersiveMode, PendingImmersiveEntry},
};

pub(super) fn enter_guest_view_for_pending_guest_subject(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    guest_subjects: Query<(), With<Guest>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller_entity, pending_entry)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::GuestView, &pending_entries)
    else {
        return;
    };
    let Some(subject_entity) = require_subject_from_pending_immersive_mode_entry(
        controller_entity,
        pending_entry,
        &mut commands,
    ) else {
        return;
    };
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    if guest_subjects.get(subject_entity).is_err() {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::WrongSubject,
            &mut commands,
        );
        return;
    }
    let Some((interaction_tool, camera_definition, lock_subject, restore_camera_on_exit)) =
        resolve_required_authored_interaction_policy(
            ImmersiveMode::GuestView,
            controller_entity,
            pending_entry,
            world_definitions,
            &mut commands,
        )
    else {
        return;
    };
    if !commit_validated_immersive_mode_entry(
        controller_entity,
        subject_entity,
        pending_entry,
        &zoo_cameras,
        GuestViewControl,
        Some(interaction_tool),
        CameraMode::FirstPerson(subject_entity),
        camera_definition,
        lock_subject,
        restore_camera_on_exit,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    ) {
        return;
    }
    commands.entity(controller_entity).insert((
        GuestViewTileCount::default(),
        calculate_initial_first_person_view_angles(world_definitions, camera_definition),
    ));
    attach_authored_immersive_mode_consumer_policies(
        world_definitions,
        ImmersiveMode::GuestView,
        controller_entity,
        &mut commands,
    );
}

pub(super) fn enter_first_person_mode_for_pending_navigable_subject(
    pending_entries: Query<(Entity, &PendingImmersiveEntry)>,
    navigable_subjects: Query<(), (Or<(With<NavAgent>, With<TransportVehicle>)>, Without<Dead>)>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    zoo_cameras: Query<Entity, With<ZooCamera>>,
    mut commands: Commands,
    mut camera_mode_requests: MessageWriter<SetCameraMode>,
    mut entered_messages: MessageWriter<ImmersiveModeEntered>,
) {
    let Some((controller_entity, pending_entry)) =
        find_pending_entry_for_immersive_mode(ImmersiveMode::FirstPerson, &pending_entries)
    else {
        return;
    };
    let Some(subject_entity) = require_subject_from_pending_immersive_mode_entry(
        controller_entity,
        pending_entry,
        &mut commands,
    ) else {
        return;
    };
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::MissingTool,
            &mut commands,
        );
        return;
    };
    if navigable_subjects.get(subject_entity).is_err() {
        record_pending_immersive_mode_entry_failure(
            controller_entity,
            pending_entry,
            ModeEntryFailure::WrongSubject,
            &mut commands,
        );
        return;
    }
    let Some((interaction_tool, camera_definition, lock_subject, restore_camera_on_exit)) =
        resolve_required_authored_interaction_policy(
            ImmersiveMode::FirstPerson,
            controller_entity,
            pending_entry,
            world_definitions,
            &mut commands,
        )
    else {
        return;
    };
    if !commit_validated_immersive_mode_entry(
        controller_entity,
        subject_entity,
        pending_entry,
        &zoo_cameras,
        FirstPersonControl,
        Some(interaction_tool),
        CameraMode::FirstPerson(subject_entity),
        camera_definition,
        lock_subject,
        restore_camera_on_exit,
        &mut commands,
        &mut camera_mode_requests,
        &mut entered_messages,
    ) {
        return;
    }
    commands
        .entity(controller_entity)
        .insert(calculate_initial_first_person_view_angles(
            world_definitions,
            camera_definition,
        ));
    attach_authored_immersive_mode_consumer_policies(
        world_definitions,
        ImmersiveMode::FirstPerson,
        controller_entity,
        &mut commands,
    );
}

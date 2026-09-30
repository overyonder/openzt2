use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::super::{
    camera_control_message_types::{
        AimFreeCameraAt, FocusCamera, PositionFreeCamera, RestoreCameraMode, SetCameraMode,
    },
    camera_runtime_state_types::{
        CameraBounds, CameraDefinition, CameraEasing, CameraIntent, CameraMode, CameraMouseLook,
        CameraReturnState, CameraTransition, CameraTuning, OverheadRig,
        RemoveReturnStateOnComplete, ZooCamera,
    },
    math::advance_transition,
};
use super::camera_pose_comparison::camera_pose_materially_changed;

pub(in crate::plugins::camera) fn clear_camera_input_on_return(
    mut commands: Commands,
    mut requests: MessageReader<RestoreCameraMode>,
    mut cameras: Query<(Entity, &mut CameraIntent), With<ZooCamera>>,
) {
    let mut returning = false;
    for request in requests.read() {
        returning |= requested_duration_is_valid(request.transition_seconds);
    }
    if !returning {
        return;
    }
    // Escape and mode-owner exits must release the same held controls as Overhead.
    for (camera, mut intent) in &mut cameras {
        *intent = CameraIntent::default();
        commands.entity(camera).remove::<CameraMouseLook>();
    }
}

pub(in crate::plugins::camera) fn capture_camera_return_state(
    mut commands: Commands,
    mut requests: MessageReader<SetCameraMode>,
    camera: Single<
        (
            Entity,
            &CameraMode,
            &CameraDefinition,
            &Transform,
            &OverheadRig,
            Option<&CameraReturnState>,
        ),
        With<ZooCamera>,
    >,
    subjects: Query<&GlobalTransform, Without<ZooCamera>>,
) {
    if camera.5.is_some() || *camera.1 != CameraMode::Overhead {
        return;
    }
    if requests.read().any(|request| {
        request.mode.is_immersive()
            && requested_duration_is_valid(request.transition_seconds)
            && mode_target_exists(request.mode, &subjects)
    }) {
        commands.entity(camera.0).insert(CameraReturnState {
            mode: *camera.1,
            definition: camera.2 .0,
            transform: *camera.3,
            overhead: camera.4.clone(),
        });
    }
}

pub(in crate::plugins::camera) fn begin_camera_transition(
    mut commands: Commands,
    mut modes: MessageReader<SetCameraMode>,
    mut focus_requests: MessageReader<FocusCamera>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut camera: Single<
        (
            Entity,
            &Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            &CameraBounds,
            &CameraTuning,
        ),
        With<ZooCamera>,
    >,
    subjects: Query<&GlobalTransform, Without<ZooCamera>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in modes.read() {
        if !requested_duration_is_valid(request.transition_seconds) {
            continue;
        }
        let Some(definition) = definitions.find_camera(request.definition) else {
            continue;
        };
        let offset = Vec3::new(
            f32::from(definition.offset_m[0]),
            f32::from(definition.offset_m[1]) + camera.6.parenting_offset_m,
            f32::from(definition.offset_m[2]),
        );
        let Some(to) = mode_target(request.mode, camera.1, &subjects, offset) else {
            continue;
        };
        *camera.2 = request.mode;
        camera.3 .0 = request.definition;
        commands
            .entity(camera.0)
            .remove::<RemoveReturnStateOnComplete>();
        apply_requested_transition(
            &mut commands,
            camera.0,
            camera.1,
            to,
            request.transition_seconds,
        );
    }
    // `capture_camera_return_state` runs first so it can observe the original
    // mode. If every request was rejected, discard that speculative snapshot;
    // processing the whole batch first avoids an invalid request erasing a
    // later valid immersive transition in the same frame.
    if *camera.2 == CameraMode::Overhead {
        commands.entity(camera.0).remove::<CameraReturnState>();
    }

    for request in focus_requests.read() {
        if *camera.2 != CameraMode::Overhead
            || !request.world.is_finite()
            || !requested_duration_is_valid(request.transition_seconds)
        {
            continue;
        }
        let previous_focus = camera.4.focus;
        camera.4.focus = request.world.xz().clamp(camera.5.min, camera.5.max);
        let delta = camera.4.focus - previous_focus;
        let mut to = *camera.1;
        to.translation += Vec3::new(delta.x, 0.0, delta.y);
        apply_requested_transition(
            &mut commands,
            camera.0,
            camera.1,
            to,
            request.transition_seconds,
        );
    }
}

/// Applies final floating-camera facts straight to Bevy's camera entity.
///
/// Position is consumed before facing so a pair emitted in one update computes
/// its orientation from the final eye point, matching the original observable
/// result without caching the original event object.
pub(in crate::plugins::camera) fn apply_free_camera_pose(
    mut positions: MessageReader<PositionFreeCamera>,
    mut targets: MessageReader<AimFreeCameraAt>,
    mut camera: Single<(&CameraMode, &mut Transform), With<ZooCamera>>,
) {
    if *camera.0 != CameraMode::Free {
        return;
    }

    let mut next = *camera.1;
    if let Some(world) = positions
        .read()
        .map(|request| request.world)
        .filter(|world| world.is_finite())
        .last()
    {
        next.translation = world;
    }

    if let Some(world) = targets
        .read()
        .map(|request| request.world)
        .filter(|world| {
            world.is_finite() && world.distance_squared(next.translation) > f32::EPSILON
        })
        .last()
    {
        next.look_at(world, Vec3::Y);
    }
    if camera_pose_materially_changed(&camera.1, &next) {
        *camera.1 = next;
    }
}

pub(in crate::plugins::camera) fn restore_camera_return_state(
    mut commands: Commands,
    mut requests: MessageReader<RestoreCameraMode>,
    mut camera: Single<
        (
            Entity,
            &Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            &CameraReturnState,
        ),
        With<ZooCamera>,
    >,
) {
    for request in requests.read() {
        if !requested_duration_is_valid(request.transition_seconds) {
            continue;
        }
        *camera.2 = camera.5.mode;
        camera.3 .0 = camera.5.definition;
        *camera.4 = camera.5.overhead.clone();
        if valid_duration(request.transition_seconds).is_some() {
            apply_requested_transition(
                &mut commands,
                camera.0,
                camera.1,
                camera.5.transform,
                request.transition_seconds,
            );
            commands
                .entity(camera.0)
                .insert(RemoveReturnStateOnComplete);
        } else if request.transition_seconds.is_none() {
            commands
                .entity(camera.0)
                .insert(camera.5.transform)
                .remove::<CameraTransition>()
                .remove::<RemoveReturnStateOnComplete>()
                .remove::<CameraReturnState>();
        }
    }
}

pub(in crate::plugins::camera) fn advance_camera_transition(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cameras: Query<
        (
            Entity,
            &mut Transform,
            &mut CameraTransition,
            Option<&RemoveReturnStateOnComplete>,
        ),
        With<ZooCamera>,
    >,
) {
    for (entity, mut transform, mut transition, remove_return) in &mut cameras {
        if advance_transition(&mut transition, &mut transform, time.delta_secs()) {
            let mut entity_commands = commands.entity(entity);
            entity_commands.remove::<CameraTransition>();
            if remove_return.is_some() {
                entity_commands
                    .remove::<RemoveReturnStateOnComplete>()
                    .remove::<CameraReturnState>();
            }
        }
    }
}

pub(in crate::plugins::camera) fn validate_camera_mode_targets(
    mut commands: Commands,
    mut camera: Single<
        (
            Entity,
            &mut Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            Option<&CameraReturnState>,
        ),
        With<ZooCamera>,
    >,
    subjects: Query<(), Without<ZooCamera>>,
) {
    let Some(subject) = camera.2.subject() else {
        return;
    };
    if subjects.get(subject).is_ok() {
        return;
    }

    if let Some(return_state) = camera.5 {
        *camera.1 = return_state.transform;
        *camera.2 = return_state.mode;
        camera.3 .0 = return_state.definition;
        *camera.4 = return_state.overhead.clone();
        commands.entity(camera.0).remove::<CameraReturnState>();
    } else {
        *camera.2 = CameraMode::Overhead;
    }
    commands
        .entity(camera.0)
        .remove::<CameraTransition>()
        .remove::<RemoveReturnStateOnComplete>();
}

pub(super) fn mode_target(
    mode: CameraMode,
    current: &Transform,
    subjects: &Query<&GlobalTransform, Without<ZooCamera>>,
    local_offset: Vec3,
) -> Option<Transform> {
    match mode {
        CameraMode::Overhead | CameraMode::Free => Some(*current),
        CameraMode::Follow(entity) => subjects
            .get(entity)
            .ok()
            .map(|subject| (*current).looking_at(subject.transform_point(local_offset), Vec3::Y)),
        CameraMode::FirstPerson(entity) => subjects.get(entity).ok().map(|subject| {
            let mut transform = subject.compute_transform();
            transform.translation = subject.transform_point(local_offset);
            transform
        }),
    }
}

pub(super) fn mode_target_exists(
    mode: CameraMode,
    subjects: &Query<&GlobalTransform, Without<ZooCamera>>,
) -> bool {
    mode.subject()
        .is_none_or(|entity| subjects.get(entity).is_ok())
}

pub(super) fn apply_requested_transition(
    commands: &mut Commands,
    entity: Entity,
    from: &Transform,
    to: Transform,
    duration: Option<f32>,
) {
    match valid_duration(duration) {
        Some(duration) => {
            commands.entity(entity).insert(CameraTransition {
                from: *from,
                to,
                elapsed: 0.0,
                duration,
                easing: CameraEasing::SmoothStep,
            });
        }
        None if duration.is_none() => {
            commands
                .entity(entity)
                .insert(to)
                .remove::<CameraTransition>();
        }
        None => {}
    }
}

pub(super) fn valid_duration(duration: Option<f32>) -> Option<f32> {
    duration.filter(|value| value.is_finite() && *value > 0.0)
}

pub(super) fn requested_duration_is_valid(duration: Option<f32>) -> bool {
    duration.is_none() || valid_duration(duration).is_some()
}

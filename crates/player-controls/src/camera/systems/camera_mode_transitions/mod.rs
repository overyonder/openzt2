use super::camera_definition_hydration::camera_runtime_components_from_authored_definition;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    world_membership_types::WorldMember, world_terrain_hydration::WorldTerrainHorizontalBounds,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::super::camera_control_message_types::AimFreeCameraAt;
use super::super::camera_control_message_types::FocusCamera;
use super::super::camera_control_message_types::PositionFreeCamera;
use super::super::camera_control_message_types::RestoreCameraMode;
use super::super::camera_control_message_types::SetCameraMode;
use super::super::camera_runtime_state_types::CameraDefinition;
use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::CameraMouseLook;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::{advance_transition, clamp_rig, overhead_pose};
use super::camera_pose_comparison::camera_pose_materially_changed;
use crate::camera::camera_execution_types::CameraBounds;
use crate::camera::camera_execution_types::CameraEasing;
use crate::camera::camera_execution_types::CameraIntent;
use crate::camera::camera_execution_types::CameraReturnState;
use crate::camera::camera_execution_types::CameraTransition;
use crate::camera::camera_execution_types::CameraTuning;
use crate::camera::camera_execution_types::{
    MapOverheadCameraDefinition, RemoveReturnStateOnComplete,
};

#[derive(SystemParam)]
pub(crate) struct CameraDefinitionSources<'w, 's> {
    authored: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    active: Option<Res<'w, WorldDefinitions>>,
    graphics: Option<Res<'w, GraphicsSettings>>,
    worlds: Query<'w, 's, &'static WorldTerrainHorizontalBounds>,
}

impl CameraDefinitionSources<'_, '_> {
    fn prepare(
        &self,
        definition: AssetId,
        membership: Option<&WorldMember>,
    ) -> Option<(Projection, CameraTuning, OverheadRig, CameraBounds)> {
        let definitions = self.active.as_deref()?.get(self.authored.as_deref()?)?;
        camera_runtime_components_from_authored_definition(
            definitions.find_camera(definition)?,
            self.worlds.get(membership?.root).ok()?,
            self.graphics.as_deref()?,
        )
    }
}

pub(crate) fn begin_camera_transition(
    mut commands: Commands,
    mut modes: MessageReader<SetCameraMode>,
    mut focus_requests: MessageReader<FocusCamera>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    settings: CameraDefinitionSources,
    mut camera: Single<
        (
            Entity,
            &mut Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            &mut CameraBounds,
            &mut CameraTuning,
            Option<&CameraReturnState>,
            Option<&mut Projection>,
            Option<&WorldMember>,
        ),
        With<ZooCamera>,
    >,
    subjects: Query<&GlobalTransform, Without<ZooCamera>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let initial_definition = camera.3 .0;
    let mut has_return_state = camera.7.is_some();
    let mut entered_overhead = false;
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
        let Some(to) = mode_target(request.mode, &camera.1, &subjects, offset) else {
            continue;
        };
        if *camera.2 == CameraMode::Overhead && request.mode.is_immersive() && !has_return_state {
            commands.entity(camera.0).insert(CameraReturnState {
                mode: *camera.2,
                definition: camera.3 .0,
                transform: *camera.1,
                overhead: camera.4.clone(),
            });
            has_return_state = true;
        }
        entered_overhead = request.mode == CameraMode::Overhead;
        *camera.2 = request.mode;
        camera.3 .0 = request.definition;
        commands
            .entity(camera.0)
            .remove::<RemoveReturnStateOnComplete>();
        let entity = camera.0;
        apply_requested_transition(
            &mut commands,
            entity,
            &mut camera.1,
            to,
            request.transition_seconds,
        );
    }
    if entered_overhead && has_return_state {
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
        let entity = camera.0;
        apply_requested_transition(
            &mut commands,
            entity,
            &mut camera.1,
            to,
            request.transition_seconds,
        );
        if request.transition_seconds.is_none() && has_return_state {
            commands
                .entity(entity)
                .remove::<CameraReturnState>()
                .remove::<RemoveReturnStateOnComplete>();
        }
    }
    if initial_definition != camera.3 .0 {
        if let Some((projection, tuning, _, bounds)) = settings.prepare(camera.3 .0, camera.9) {
            clamp_rig(&mut camera.4, &tuning, &bounds);
            *camera.5 = bounds;
            *camera.6 = tuning;
            if let Some(projection_component) = camera.8.as_deref_mut() {
                *projection_component = projection;
            }
        }
    }
}

/// Applies final floating-camera facts straight to Bevy's camera entity.
///
/// Position is consumed before facing so a pair emitted in one update computes
/// its orientation from the final eye point, matching the original observable
/// result without caching the original event object.
pub(crate) fn apply_free_camera_pose(
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

pub(crate) fn apply_camera_return(
    mut commands: Commands,
    mut messages: MessageReader<RestoreCameraMode>,
    settings: CameraDefinitionSources,
    mut camera: Single<
        (
            Entity,
            &mut Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            Option<&mut CameraIntent>,
            Has<CameraMouseLook>,
            Option<&CameraReturnState>,
            Option<&MapOverheadCameraDefinition>,
            Option<&WorldMember>,
            Option<&mut Projection>,
            Option<&mut CameraTuning>,
            Option<&mut CameraBounds>,
        ),
        With<ZooCamera>,
    >,
) {
    let mut requests = messages
        .read()
        .filter(|request| requested_duration_is_valid(request.transition_seconds));
    let Some(first) = requests.next() else { return };
    let entity = camera.0;
    if let Some(intent) = camera.5.as_deref_mut() {
        *intent = CameraIntent::default();
    }
    if camera.6 {
        commands.entity(entity).remove::<CameraMouseLook>();
    }

    let (duration, mode, definition, transform, mut rig, prepared, restore_snapshot) =
        if let Some(snapshot) = camera.7 {
            let last = requests.last().unwrap_or(first);
            (
                last.transition_seconds,
                snapshot.mode,
                snapshot.definition,
                snapshot.transform,
                snapshot.overhead.clone(),
                settings.prepare(snapshot.definition, camera.9),
                true,
            )
        } else {
            if *camera.2 == CameraMode::Overhead {
                return;
            }
            let Some(authored) = camera.8 else { return };
            let Some((projection, tuning, mut rig, bounds)) =
                settings.prepare(authored.0, camera.9)
            else {
                return;
            };
            rig.focus = camera.4.focus;
            rig.height_m = camera.4.height_m;
            rig.yaw = camera.4.yaw;
            clamp_rig(&mut rig, &tuning, &bounds);
            let transform = overhead_pose(&rig);
            (
                first.transition_seconds,
                CameraMode::Overhead,
                authored.0,
                transform,
                rig.clone(),
                Some((projection, tuning, rig, bounds)),
                false,
            )
        };
    if let Some((projection, tuning, _, bounds)) = prepared {
        if restore_snapshot {
            clamp_rig(&mut rig, &tuning, &bounds);
        }
        if let Some(current) = camera.10.as_deref_mut() {
            *current = projection;
        }
        if let Some(current) = camera.11.as_deref_mut() {
            *current = tuning;
        }
        if let Some(current) = camera.12.as_deref_mut() {
            *current = bounds;
        }
    }
    camera.2.set_if_neq(mode);
    if camera.3 .0 != definition {
        camera.3 .0 = definition;
    }
    *camera.4 = rig;
    apply_requested_transition(&mut commands, entity, &mut camera.1, transform, duration);
    let mut commands = commands.entity(entity);
    if restore_snapshot && duration.is_some() {
        commands.insert(RemoveReturnStateOnComplete);
    } else {
        commands.remove::<(RemoveReturnStateOnComplete, CameraReturnState)>();
    }
}

pub(crate) fn advance_camera_transition(
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

pub(crate) fn request_return_when_camera_target_disappears(
    camera: Single<&CameraMode, With<ZooCamera>>,
    subjects: Query<(), Without<ZooCamera>>,
    mut returns: MessageWriter<RestoreCameraMode>,
) {
    if camera
        .subject()
        .is_some_and(|subject| !subjects.contains(subject))
    {
        returns.write(RestoreCameraMode {
            transition_seconds: None,
        });
    }
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

pub(super) fn apply_requested_transition(
    commands: &mut Commands,
    entity: Entity,
    from: &mut Transform,
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
            *from = to;
            commands.entity(entity).remove::<CameraTransition>();
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

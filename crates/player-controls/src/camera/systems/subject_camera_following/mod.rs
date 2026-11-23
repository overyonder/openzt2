use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::super::camera_runtime_state_types::CameraDefinition;
use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::StandaloneCameraSubject;
use super::super::camera_runtime_state_types::ZooCamera;
use super::camera_pose_comparison::camera_pose_materially_changed;
use crate::camera::camera_execution_types::CameraTransition;
use crate::camera::camera_execution_types::CameraTuning;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, ImmersiveCameraSessions, ImmersiveSubject,
};

/// Follows the subject after the entry transition. First-person mode is handled
/// by immersive modes, which also applies eye height and collision settings.
pub(crate) fn update_subject_camera_pose(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    subjects: Query<&GlobalTransform, Without<ZooCamera>>,
    sessions: Query<&ImmersiveSubject, With<ActiveImmersiveMode>>,
    mut camera: Single<
        (
            &CameraMode,
            &CameraDefinition,
            &CameraTuning,
            &mut Transform,
            Option<&StandaloneCameraSubject>,
            Option<&ImmersiveCameraSessions>,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    if *camera.0 != CameraMode::Follow {
        return;
    }
    let subject = camera.4.map(|standalone| standalone.0).or_else(|| {
        camera
            .5?
            .iter()
            .find_map(|session| sessions.get(session).ok().map(|subject| subject.0))
    });
    let Some(subject_transform) = subject.and_then(|subject| subjects.get(subject).ok()) else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(definition) = definitions.find_camera(camera.1 .0) else {
        return;
    };
    let offset = Vec3::new(
        f32::from(definition.offset_m[0]),
        f32::from(definition.offset_m[1]) + camera.2.parenting_offset_m,
        f32::from(definition.offset_m[2]),
    );
    let target = subject_transform.transform_point(offset);
    if camera.3.translation.distance_squared(target) > f32::EPSILON {
        let next = (*camera.3).looking_at(target, Vec3::Y);
        if camera_pose_materially_changed(&camera.3, &next) {
            *camera.3 = next;
        }
    }
}

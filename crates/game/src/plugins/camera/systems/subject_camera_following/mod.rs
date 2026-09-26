use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    super::camera_runtime_state_types::{
        CameraDefinition, CameraMode, CameraTransition, CameraTuning, ZooCamera,
    },
    camera_pose_comparison::camera_pose_materially_changed,
};

/// Follows the subject after the entry transition. First-person mode is handled
/// by immersive modes, which also applies eye height and collision settings.
pub(in crate::plugins::camera) fn update_subject_camera_pose(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    subjects: Query<&GlobalTransform, Without<ZooCamera>>,
    mut camera: Single<
        (
            &CameraMode,
            &CameraDefinition,
            &CameraTuning,
            &mut Transform,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let subject = match *camera.0 {
        CameraMode::Follow(subject) => subject,
        CameraMode::Overhead | CameraMode::FirstPerson(_) | CameraMode::Free => return,
    };
    let Ok(subject_transform) = subjects.get(subject) else {
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
    match *camera.0 {
        CameraMode::Follow(_) => {
            let target = subject_transform.transform_point(offset);
            if camera.3.translation.distance_squared(target) > f32::EPSILON {
                let next = (*camera.3).looking_at(target, Vec3::Y);
                if camera_pose_materially_changed(&camera.3, &next) {
                    *camera.3 = next;
                }
            }
        }
        CameraMode::Overhead | CameraMode::FirstPerson(_) | CameraMode::Free => unreachable!(),
    }
}

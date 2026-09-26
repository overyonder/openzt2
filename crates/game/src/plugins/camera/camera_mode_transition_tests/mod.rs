use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    camera_control_message_types::{
        AimFreeCameraAt, PositionFreeCamera, RestoreCameraMode, SetCameraMode,
    },
    camera_runtime_state_types::{
        CameraDefinition, CameraMode, CameraReturnState, OverheadRig, ZooCamera,
    },
    systems::camera_mode_transitions::{
        apply_free_camera_pose, capture_camera_return_state, restore_camera_return_state,
    },
};

fn overhead_camera_rig_for_mode_transition_tests() -> OverheadRig {
    OverheadRig {
        focus: Vec2::ZERO,
        height_m: 0.0,
        look_at_height_m: 0.2,
        yaw: 0.0,
        pitch: 0.7,
        distance: 32.0,
        look_at_distance_m: 12.0,
        minimum_zoom_offset_m: 0.0,
        camera_ground_fit_offset_m: 0.0,
        target_ground_fit_offset_m: 0.0,
        pan_accumulators: [0.0; 4],
    }
}

fn camera_definition_identifier_with_repeated_byte(value: u8) -> AssetId {
    AssetId([value; 16])
}

#[test]
fn free_camera_pose_messages_write_only_the_authoritative_transform() {
    let mut application = App::new();
    application
        .add_message::<PositionFreeCamera>()
        .add_message::<AimFreeCameraAt>()
        .add_systems(Update, apply_free_camera_pose);
    let camera_entity = application
        .world_mut()
        .spawn((ZooCamera, CameraMode::Free, Transform::default()))
        .id();
    application.world_mut().write_message(PositionFreeCamera {
        world: Vec3::new(4.0, 5.0, 6.0),
    });
    application.world_mut().write_message(AimFreeCameraAt {
        world: Vec3::new(4.0, 5.0, 16.0),
    });
    application.update();
    let camera_transform = application
        .world()
        .entity(camera_entity)
        .get::<Transform>()
        .unwrap();
    assert_eq!(camera_transform.translation, Vec3::new(4.0, 5.0, 6.0));
    assert!(camera_transform.forward().dot(Vec3::Z) > 0.999);
}

#[test]
fn immersive_mode_captures_once_and_restore_is_exact() {
    let mut application = App::new();
    application
        .add_message::<SetCameraMode>()
        .add_message::<RestoreCameraMode>()
        .add_systems(Update, capture_camera_return_state)
        .add_systems(
            Update,
            restore_camera_return_state.after(capture_camera_return_state),
        );
    let original_camera_transform = Transform::from_xyz(4.0, 20.0, 9.0);
    let original_overhead_camera_rig = overhead_camera_rig_for_mode_transition_tests();
    let camera_entity = application
        .world_mut()
        .spawn((
            ZooCamera,
            CameraMode::Overhead,
            CameraDefinition(camera_definition_identifier_with_repeated_byte(1)),
            original_camera_transform,
            original_overhead_camera_rig.clone(),
        ))
        .id();
    let immersive_subject_entity = application
        .world_mut()
        .spawn(GlobalTransform::IDENTITY)
        .id();

    application.world_mut().write_message(SetCameraMode {
        mode: CameraMode::FirstPerson(immersive_subject_entity),
        definition: camera_definition_identifier_with_repeated_byte(2),
        transition_seconds: None,
    });
    application.update();
    let stored_return_state = application
        .world()
        .get::<CameraReturnState>(camera_entity)
        .unwrap();
    assert_eq!(stored_return_state.mode, CameraMode::Overhead);
    assert_eq!(
        stored_return_state.definition,
        camera_definition_identifier_with_repeated_byte(1)
    );
    assert_eq!(stored_return_state.transform, original_camera_transform);
    assert_eq!(stored_return_state.overhead, original_overhead_camera_rig);

    application.world_mut().entity_mut(camera_entity).insert((
        CameraMode::FirstPerson(immersive_subject_entity),
        CameraDefinition(camera_definition_identifier_with_repeated_byte(2)),
        Transform::from_xyz(100.0, 3.0, -20.0),
    ));
    application.world_mut().write_message(RestoreCameraMode {
        transition_seconds: None,
    });
    application.update();
    assert_eq!(
        *application
            .world()
            .get::<CameraMode>(camera_entity)
            .unwrap(),
        CameraMode::Overhead
    );
    assert_eq!(
        application
            .world()
            .get::<CameraDefinition>(camera_entity)
            .unwrap()
            .0,
        camera_definition_identifier_with_repeated_byte(1)
    );
    assert_eq!(
        *application.world().get::<Transform>(camera_entity).unwrap(),
        original_camera_transform
    );
    assert_eq!(
        *application
            .world()
            .get::<OverheadRig>(camera_entity)
            .unwrap(),
        original_overhead_camera_rig
    );
    assert!(application
        .world()
        .get::<CameraReturnState>(camera_entity)
        .is_none());
}

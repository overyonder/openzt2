use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::camera_control_message_types::AimFreeCameraAt;
use super::camera_control_message_types::PositionFreeCamera;
use super::camera_control_message_types::RestoreCameraMode;
use super::camera_control_message_types::SetCameraMode;
use super::camera_runtime_state_types::CameraDefinition;
use super::camera_runtime_state_types::CameraMode;
use super::camera_runtime_state_types::OverheadRig;
use super::camera_runtime_state_types::ZooCamera;
use super::systems::camera_mode_transitions::apply_camera_return;
use super::systems::camera_mode_transitions::apply_free_camera_pose;
use super::systems::camera_mode_transitions::begin_camera_transition;
use crate::camera::camera_execution_types::CameraReturnState;

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
    use crate::camera::camera_execution_types::CameraBounds;
    use openzt2_game_assets::world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    };
    use openzt2_game_data::world_definitions::{
        camera_definitions::{CameraProjectionKind, CameraTuningDefinition},
        document::WorldDefinitionDocument,
    };

    let mut application = App::new();
    application
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .add_message::<super::camera_control_message_types::FocusCamera>()
        .add_message::<SetCameraMode>()
        .add_message::<RestoreCameraMode>()
        .add_systems(Update, begin_camera_transition)
        .add_systems(Update, apply_camera_return.after(begin_camera_transition));
    let document = WorldDefinitionDocument {
        cameras: vec![CameraTuningDefinition {
            id: camera_definition_identifier_with_repeated_byte(2),
            projection: CameraProjectionKind::Perspective,
            fov_y_radians: 1.0,
            fixed_aspect_ratio: 1.0,
            vertical_view_per_zoom: 0.0,
            near_m: 0.1,
            far_m: [100.0; 3],
            offset_m: [0.0; 3],
            pitch_radians: [-1.0, 1.0],
            yaw_radians: [-1.0, 1.0],
            minimum_zoom_m: 1.0,
            maximum_zoom_m: [10.0; 3],
            collision_radius_cm: 0,
            initial_pitch_radians: 0.0,
            initial_zoom_m: 3.0,
            look_at_distance_m: 1.0,
            parenting_offset_m: 0.0,
            pan_speed_mps: 1.0,
            turn_speed_rps: 1.0,
            zoom_speed_mps: 1.0,
            pan_start_rate: 1.0,
            pan_stop_rate: -1.0,
            ground_buffer_m: 0.0,
            soft_fit_distance_m: 0.0,
            soft_fit_speed_mps: 1.0,
            no_tilt_on_fit: false,
            edge_scroll: false,
            zoom_speed_samples: [[0.0; 2]; 8],
            zoom_speed_sample_count: 0,
            sub_zero_zoom_multiplier: 1.0,
            ground_acceleration_mps2: 0.0,
        }],
        ..default()
    };
    let handle = application
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    application
        .world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
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
            CameraBounds {
                min: Vec2::splat(-100.0),
                max: Vec2::splat(100.0),
                camera_clearance_m: 0.0,
            },
            super::camera_calculation_tests::camera_tuning_for_calculation_tests(),
        ))
        .id();
    let immersive_subject_entity = application
        .world_mut()
        .spawn(GlobalTransform::IDENTITY)
        .id();

    application.world_mut().write_message(SetCameraMode {
        mode: CameraMode::FirstPerson,
        subject: Some(immersive_subject_entity),
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
        CameraMode::FirstPerson,
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

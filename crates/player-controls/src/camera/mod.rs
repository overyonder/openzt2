//! Zoo camera controls.

pub(crate) mod camera_execution_types;

use openzt2_interface_state::camera::camera_control_message_types;
use openzt2_interface_state::camera::camera_runtime_state_types;
mod fixed_aspect_perspective_projection;
mod math;
mod photo_zoom;
pub mod systems;
mod ui_actions;
use openzt2_interface_state::camera::world_pointer_ray_types;

use camera_control_message_types::{
    AimFreeCameraAt, CameraMoved, FocusCamera, PositionFreeCamera, RestoreCameraMode, SetCameraMode,
};
use systems::{
    camera_action_intent_projection::project_camera_actions_into_camera_intent,
    camera_definition_hydration::apply_view_distance_graphics_setting_to_zoo_camera_far_planes,
    camera_mode_transitions::{
        advance_camera_transition, apply_camera_return, apply_free_camera_pose,
        begin_camera_transition, request_return_when_camera_target_disappears,
    },
    camera_motion_reporting::report_camera_motion,
    default_ui_camera_routing::route_default_ui_camera,
    ground_fitted_free_camera_initialization::initialize_ground_fitted_free_camera_from_overhead_focus,
    mouse_look_cursor_projection::project_camera_mouse_look_onto_primary_window_cursor,
    overhead_camera_advancement::{advance_overhead_camera, CameraFittingSurfaceSample},
    overhead_camera_zoom_policy::apply_overhead_camera_zoom_graphics_setting,
    pointer_ray_projection::project_pointer_ray,
    subject_camera_following::update_subject_camera_pose,
    zoo_camera_spawning::spawn_zoo_camera,
};
use world_pointer_ray_types::WorldPointerRay;

use bevy::prelude::*;

use openzt2_world_state::application_schedule::GameSet;

pub struct ZooCameraPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZooCameraPoseUpdateSystems;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CameraUiActionRouting;

impl Plugin for ZooCameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<camera_runtime_state_types::ZooCamera, CameraFittingSurfaceSample>()
            .init_resource::<WorldPointerRay>()
            .add_message::<SetCameraMode>()
            .add_message::<RestoreCameraMode>()
            .add_message::<FocusCamera>()
            .add_message::<PositionFreeCamera>()
            .add_message::<AimFreeCameraAt>()
            .add_message::<CameraMoved>()
            .add_systems(
                Update,
                (
                    ui_actions::route_camera_ui_actions.in_set(CameraUiActionRouting),
                    project_camera_actions_into_camera_intent
                        .after(ui_actions::route_camera_ui_actions),
                    begin_camera_transition.after(crate::immersive_modes::ImmersiveModeTransitions),
                    apply_free_camera_pose.after(begin_camera_transition),
                    apply_camera_return
                        .after(apply_free_camera_pose)
                        .after(project_camera_actions_into_camera_intent),
                    project_camera_mouse_look_onto_primary_window_cursor.after(apply_camera_return),
                    request_return_when_camera_target_disappears.after(begin_camera_transition).before(apply_camera_return),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    spawn_zoo_camera,
                    route_default_ui_camera.after(spawn_zoo_camera),
                    initialize_ground_fitted_free_camera_from_overhead_focus
                        .after(spawn_zoo_camera)
                        .before(advance_camera_transition),
                    apply_overhead_camera_zoom_graphics_setting
                        .after(spawn_zoo_camera),
                    apply_view_distance_graphics_setting_to_zoo_camera_far_planes
                        .after(spawn_zoo_camera)
                        .after(photo_zoom::apply_photo_camera_zoom),
                    advance_overhead_camera.after(apply_overhead_camera_zoom_graphics_setting),
                    photo_zoom::apply_photo_camera_zoom
                        .after(spawn_zoo_camera),
                    advance_camera_transition
                        .after(spawn_zoo_camera),
                    update_subject_camera_pose
                        .after(advance_overhead_camera)
                        .after(advance_camera_transition),
                )
                    .in_set(GameSet::Presentation)
                    .in_set(ZooCameraPoseUpdateSystems),
            )
            .add_systems(
                PreUpdate,
                // Pick against the last presented camera pose with this frame's pointer.
                project_pointer_ray.after(bevy::picking::PickingSystems::ProcessInput),
            )
            .add_systems(
                PostUpdate,
                report_camera_motion.in_set(GameSet::Diagnostics),
            );
    }
}

#[cfg(test)]
mod camera_calculation_tests;
#[cfg(test)]
mod camera_mode_transition_tests;

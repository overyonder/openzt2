//! Zoo camera controls.

pub(crate) mod camera_control_message_types;
pub(crate) mod camera_runtime_state_types;
mod fixed_aspect_perspective_projection;
mod math;
mod photo_zoom;
mod systems;
mod ui_actions;
pub(crate) mod world_pointer_ray_types;

use camera_control_message_types::{
    AimFreeCameraAt, CameraMoved, FocusCamera, PositionFreeCamera, RestoreCameraMode, SetCameraMode,
};
use systems::{
    camera_action_intent_projection::project_camera_actions_into_camera_intent,
    camera_definition_hydration::apply_authored_camera_definition_to_runtime_components,
    camera_mode_transitions::{
        advance_camera_transition, apply_free_camera_pose, begin_camera_transition,
        capture_camera_return_state, clear_camera_input_on_return, restore_camera_return_state,
        validate_camera_mode_targets,
    },
    camera_motion_reporting::report_camera_motion,
    default_ui_camera_routing::route_default_ui_camera,
    ground_fitted_free_camera_initialization::initialize_ground_fitted_free_camera_from_overhead_focus,
    mouse_look_cursor_projection::project_camera_mouse_look_onto_primary_window_cursor,
    overhead_camera_advancement::advance_overhead_camera,
    overhead_camera_return_without_snapshot::restore_authored_overhead_camera_without_snapshot,
    overhead_camera_zoom_policy::apply_overhead_camera_zoom_graphics_setting,
    pointer_ray_projection::project_pointer_ray,
    subject_camera_following::update_subject_camera_pose,
    zoo_camera_spawning::spawn_zoo_camera,
};
use world_pointer_ray_types::WorldPointerRay;

use bevy::{prelude::*, transform::TransformSystems};

use crate::application_schedule::GameSet;

pub(crate) struct ZooCameraPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ZooCameraPoseUpdateSystems;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CameraUiActionRouting;

impl Plugin for ZooCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldPointerRay>()
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
                    project_camera_mouse_look_onto_primary_window_cursor
                        .after(clear_camera_input_on_return),
                    capture_camera_return_state
                        .after(crate::plugins::immersive_modes::ImmersiveModeTransitions),
                    begin_camera_transition.after(capture_camera_return_state),
                    apply_free_camera_pose
                        .after(begin_camera_transition)
                        .before(restore_authored_overhead_camera_without_snapshot),
                    restore_camera_return_state,
                    clear_camera_input_on_return
                        .after(apply_free_camera_pose)
                        .after(project_camera_actions_into_camera_intent)
                        .before(restore_authored_overhead_camera_without_snapshot),
                    restore_authored_overhead_camera_without_snapshot
                        .before(restore_camera_return_state),
                    validate_camera_mode_targets
                        .after(begin_camera_transition)
                        .after(restore_camera_return_state),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    spawn_zoo_camera,
                    route_default_ui_camera.after(spawn_zoo_camera),
                    apply_authored_camera_definition_to_runtime_components.after(spawn_zoo_camera),
                    initialize_ground_fitted_free_camera_from_overhead_focus
                        .after(apply_authored_camera_definition_to_runtime_components)
                        .before(advance_camera_transition),
                    apply_overhead_camera_zoom_graphics_setting
                        .after(apply_authored_camera_definition_to_runtime_components),
                    advance_overhead_camera.after(apply_overhead_camera_zoom_graphics_setting),
                    photo_zoom::apply_photo_camera_zoom
                        .after(apply_authored_camera_definition_to_runtime_components),
                    advance_camera_transition
                        .after(apply_authored_camera_definition_to_runtime_components),
                    update_subject_camera_pose
                        .after(advance_overhead_camera)
                        .after(advance_camera_transition),
                )
                    .in_set(GameSet::Presentation)
                    .in_set(ZooCameraPoseUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                project_pointer_ray
                    .after(TransformSystems::Propagate)
                    .in_set(GameSet::Presentation),
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

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use super::super::camera_runtime_state_types::{CameraMouseLook, ZooCamera};

/// Hides and locks the primary window cursor while mouse-look is active.
pub(in crate::plugins::camera) fn project_camera_mouse_look_onto_primary_window_cursor(
    cameras: Query<(), (With<ZooCamera>, With<CameraMouseLook>)>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut was_active: Local<bool>,
) {
    let active = !cameras.is_empty();
    if active == *was_active {
        return;
    }
    let Ok(mut cursor) = windows.single_mut() else {
        return;
    };
    cursor.visible = !active;
    cursor.grab_mode = if active {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    *was_active = active;
}

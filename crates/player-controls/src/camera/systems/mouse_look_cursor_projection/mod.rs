use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use super::super::camera_runtime_state_types::CameraMouseLook;
use super::super::camera_runtime_state_types::ZooCamera;

pub(crate) fn project_camera_mouse_look_onto_primary_window_cursor(
    cameras: Query<(), (With<ZooCamera>, With<CameraMouseLook>)>,
    cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let mut cursor = cursor.into_inner();
    let active = !cameras.is_empty();
    let grab_mode = if active {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    cursor
        .reborrow()
        .map_unchanged(|cursor| &mut cursor.visible)
        .set_if_neq(!active);
    cursor
        .map_unchanged(|cursor| &mut cursor.grab_mode)
        .set_if_neq(grab_mode);
}

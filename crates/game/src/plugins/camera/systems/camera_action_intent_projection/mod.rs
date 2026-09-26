use bevy::prelude::*;

use crate::{
    plugins::input::input_types::{ActionRequest, GameAction},
    plugins::{
        construction::construction_tool_and_placement_policy_types::ConstructionTool,
        input::input_types::{DeviceControlAxes, PrimaryPointerInputState},
    },
};

use super::super::{
    camera_runtime_state_types::{CameraIntent, CameraMode, CameraMouseLook, ZooCamera},
    math::queue_wheel_zoom,
};

pub(in crate::plugins::camera) fn project_camera_actions_into_camera_intent(
    mut actions: MessageReader<ActionRequest>,
    axes: Res<DeviceControlAxes>,
    pointer: Res<PrimaryPointerInputState>,
    tool: Res<ConstructionTool>,
    mut cameras: Query<
        (
            &mut CameraIntent,
            &CameraMode,
            Has<CameraMouseLook>,
            Has<crate::plugins::photos::photo_capture_types::PhotoMode>,
        ),
        With<ZooCamera>,
    >,
) {
    let mut digital_turn = 0.0;
    let mut digital_zoom = 0.0;
    for request in actions.read() {
        match request.action {
            GameAction::RotateLeft if !matches!(*tool, ConstructionTool::Place(_)) => {
                digital_turn -= 1.0
            }
            GameAction::RotateRight if !matches!(*tool, ConstructionTool::Place(_)) => {
                digital_turn += 1.0
            }
            GameAction::ZoomIn => digital_zoom += 1.0,
            GameAction::ZoomOut => digital_zoom -= 1.0,
            _ => {}
        }
    }

    let turn = (axes.look.x + digital_turn).clamp(-1.0, 1.0);
    let zoom = (axes.zoom + digital_zoom).clamp(-1.0, 1.0);
    for (mut intent, mode, mouse_look, photo_mode) in &mut cameras {
        if !photo_mode {
            intent.photo_zoom_in_command = false;
            intent.photo_zoom_out_command = false;
        }
        if *mode == CameraMode::Overhead {
            let ui_directions = [
                intent.ui_pan.y.max(0.0),
                (-intent.ui_pan.y).max(0.0),
                intent.ui_pan.x.max(0.0),
                (-intent.ui_pan.x).max(0.0),
            ];
            intent.pan_directions =
                std::array::from_fn(|index| axes.pan_directions[index].max(ui_directions[index]));
            intent.turn = (turn + intent.ui_turn).clamp(-1.0, 1.0);
            intent.pitch = if mouse_look { -axes.look.y } else { 0.0 };
            intent.zoom = (zoom + intent.ui_zoom).clamp(-1.0, 1.0);
            queue_wheel_zoom(&mut intent.wheel_zoom_seconds, pointer.wheel_y);
        } else {
            intent.pan_directions = [0.0; 4];
            intent.turn = 0.0;
            intent.pitch = 0.0;
            intent.zoom = 0.0;
            intent.wheel_zoom_seconds = 0.0;
        }
    }
}

use bevy::prelude::*;

use crate::input::input_execution_types::DeviceControlAxes;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;

use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::CameraMouseLook;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::queue_wheel_zoom;
use super::super::math::WheelZoomQueue;
use crate::camera::camera_execution_types::CameraIntent;

pub(crate) fn project_camera_actions_into_camera_intent(
    mut actions: MessageReader<ActionRequest>,
    axes: Res<DeviceControlAxes>,
    pointer: Res<PrimaryPointerInputState>,
    tool: Res<ConstructionTool>,
    capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    mut cameras: Query<
        (
            &mut CameraIntent,
            &CameraMode,
            Has<CameraMouseLook>,
            Has<openzt2_interface_state::photos::photo_capture_types::PhotoMode>,
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
            if capture.wheel
                == openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture::World
            {
                queue_wheel_zoom(&mut intent.wheel_zoom, pointer.wheel_y);
            }
        } else {
            intent.pan_directions = [0.0; 4];
            intent.turn = 0.0;
            intent.pitch = 0.0;
            intent.zoom = 0.0;
            intent.wheel_zoom = WheelZoomQueue::Idle;
        }
    }
}

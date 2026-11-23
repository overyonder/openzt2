use bevy::prelude::*;

use crate::input::input_execution_types::DeviceControlAxes;
use bevy::input::mouse::AccumulatedMouseScroll;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;

use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::CameraMouseLook;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::queue_wheel_zoom;
use super::super::math::WheelZoomQueue;
use crate::camera::camera_execution_types::{CameraIntent, CameraTuning};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiEdgePanSurface;

pub(crate) fn project_camera_actions_into_camera_intent(
    mut actions: MessageReader<ActionRequest>,
    axes: Res<DeviceControlAxes>,
    scroll: Res<AccumulatedMouseScroll>,
    tool: Res<ConstructionTool>,
    placement_policy: Res<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy>,
    capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    edge_surfaces: Query<(&UiEdgePanSurface, &InheritedVisibility, &Node)>,
    mut cameras: Query<
        (
            &mut CameraIntent,
            &CameraMode,
            &CameraTuning,
            Has<CameraMouseLook>,
            Has<openzt2_interface_state::photos::photo_capture_types::PhotoMode>,
        ),
        With<ZooCamera>,
    >,
) {
    let mut digital_turn = 0.0;
    let construction_owns_rotation = matches!(*tool, ConstructionTool::Place(_))
        || matches!(*tool, ConstructionTool::Fence(_))
            && placement_policy.fence_shape != openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::FencePlacementShape::Freeform;
    let mut digital_zoom = 0.0;
    for request in actions.read() {
        match request.action {
            GameAction::RotateLeft if !construction_owns_rotation => digital_turn -= 1.0,
            GameAction::RotateRight if !construction_owns_rotation => digital_turn += 1.0,
            GameAction::ZoomIn => digital_zoom += 1.0,
            GameAction::ZoomOut => digital_zoom -= 1.0,
            _ => {}
        }
    }

    let turn = (axes.look.x + digital_turn).clamp(-1.0, 1.0);
    let zoom = (axes.zoom + digital_zoom).clamp(-1.0, 1.0);
    for (mut intent, mode, tuning, mouse_look, photo_mode) in &mut cameras {
        let mut next = *intent;
        if !photo_mode {
            next.photo_zoom_in_command = false;
            next.photo_zoom_out_command = false;
        }
        let edge = next.edge_pan.filter(|(surface, _)| {
            *mode == CameraMode::Overhead
                && tuning.edge_scroll
                && edge_surfaces
                    .get(*surface)
                    .is_ok_and(|(surface, visibility, layout)| {
                        surface.0 && visibility.get() && layout.display != Display::None
                    })
        });
        next.edge_pan = edge;
        if *mode == CameraMode::Overhead {
            let edge = edge.map_or(Vec2::ZERO, |(_, direction)| direction);
            let ui_directions = [
                next.ui_pan.y.max(edge.y).max(0.0),
                (-next.ui_pan.y).max(-edge.y).max(0.0),
                next.ui_pan.x.max(edge.x).max(0.0),
                (-next.ui_pan.x).max(-edge.x).max(0.0),
            ];
            next.pan_directions =
                std::array::from_fn(|index| axes.pan_directions[index].max(ui_directions[index]));
            next.turn = (turn + next.ui_turn).clamp(-1.0, 1.0);
            next.pitch = if mouse_look { -axes.look.y } else { 0.0 };
            next.zoom = (zoom + next.ui_zoom).clamp(-1.0, 1.0);
            if capture.wheel
                == openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture::World
            {
                queue_wheel_zoom(&mut next.wheel_zoom, scroll.delta.y);
            }
        } else {
            next.pan_directions = [0.0; 4];
            next.turn = 0.0;
            next.pitch = 0.0;
            next.zoom = 0.0;
            next.wheel_zoom = WheelZoomQueue::Idle;
        }
        intent.set_if_neq(next);
    }
}

pub(in crate::camera) fn project_authored_edge_pan_availability(
    cameras: Query<(&CameraMode, &CameraTuning), With<ZooCamera>>,
    mut surfaces: Query<&mut UiEdgePanSurface>,
) {
    let enabled = cameras
        .single()
        .is_ok_and(|(mode, tuning)| *mode == CameraMode::Overhead && tuning.edge_scroll);
    for mut surface in &mut surfaces {
        surface.set_if_neq(UiEdgePanSurface(enabled));
    }
}

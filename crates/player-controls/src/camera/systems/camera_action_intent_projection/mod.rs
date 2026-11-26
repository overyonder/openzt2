use bevy::prelude::*;

use crate::input::input_execution_types::DeviceControlAxes;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    EquippedInteractionTool, ImmersiveCamera,
};
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::pointer_capture_types::WorldWheelInput;
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
    input: Res<openzt2_interface_state::input::input_types::ActiveInputDevice>,
    mut scroll: MessageReader<WorldWheelInput>,
    tool: Res<ConstructionTool>,
    placement_policy: Res<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy>,
    commands: openzt2_ui::authored_hotkey_keyboard_activation::AuthoredHeldCameraCommands,
    immersive_modes: Query<(&ImmersiveCamera, &EquippedInteractionTool)>,
    edge_surfaces: Query<(&UiEdgePanSurface, &InheritedVisibility, &Node)>,
    mut cameras: Query<
        (
            Entity,
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
            GameAction::RotateLeft if !construction_owns_rotation && request.source != openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse => digital_turn -= 1.0,
            GameAction::RotateRight if !construction_owns_rotation && request.source != openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse => digital_turn += 1.0,
            GameAction::ZoomIn if request.source != openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse => digital_zoom += 1.0,
            GameAction::ZoomOut if request.source != openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse => digital_zoom -= 1.0,
            _ => {}
        }
    }

    let wheel_events = scroll
        .read()
        .map(|event| event.notches.trunc())
        .collect::<Vec<_>>();
    let keyboard =
        input.source == openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse;
    let zoom = (if keyboard { 0.0 } else { axes.zoom } + digital_zoom).clamp(-1.0, 1.0);
    for (camera, mut intent, mode, tuning, mouse_look, photo_mode) in &mut cameras {
        let mut next = *intent;
        use openzt2_game_data::{ui_document::document::UiDocumentRole, AssetId};
        let held = commands
            .for_role(if photo_mode {
                UiDocumentRole::PhotoMode
            } else {
                UiDocumentRole::InGameHud
            })
            .collect::<bevy::platform::collections::HashSet<_>>();
        let active = |name| held.contains(&AssetId::from_key(name));
        next.photo_zoom_in_command = photo_mode && active("photozoomin");
        next.photo_zoom_out_command = photo_mode && active("photozoomout");
        let mode_policy = immersive_modes
            .iter()
            .find_map(|(owner, tool)| (owner.0 == camera).then_some(tool.policy))
            .or_else(|| {
                (*mode == CameraMode::FirstPerson).then(|| AssetId::from_key("mode_first_person"))
            });
        let first_person = mode_policy
            .into_iter()
            .flat_map(|mode| commands.first_person(mode))
            .collect::<bevy::platform::collections::HashSet<_>>();
        use openzt2_game_data::ui_document::action::camera::UiFirstPersonCommand as Command;
        next.first_person_commands = crate::camera::camera_execution_types::FirstPersonCommands {
            forward: first_person.contains(&Command::Forward),
            backward: first_person.contains(&Command::Backward),
            turn_left: first_person.contains(&Command::TurnLeft),
            turn_right: first_person.contains(&Command::TurnRight),
            up: first_person.contains(&Command::Up),
            down: first_person.contains(&Command::Down),
            crouch: first_person.contains(&Command::Crouch),
            free_mouse_look: first_person.contains(&Command::FreeMouseLook),
        };
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
            let authored_turn = if construction_owns_rotation {
                0.0
            } else {
                f32::from(active("rright")) - f32::from(active("rleft"))
            };
            let look = if keyboard && active("mouselook") {
                axes.mouse_look
            } else {
                axes.look
            };
            next.turn = (look.x + digital_turn + next.ui_turn + authored_turn).clamp(-1.0, 1.0);
            next.pitch = if mouse_look || active("mouselook") {
                -look.y
            } else {
                0.0
            };
            let authored_zoom = f32::from(active("up")) - f32::from(active("down"));
            next.zoom = (zoom + next.ui_zoom + authored_zoom).clamp(-1.0, 1.0);
            for &notches in &wheel_events {
                queue_wheel_zoom(&mut next.wheel_zoom, notches);
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

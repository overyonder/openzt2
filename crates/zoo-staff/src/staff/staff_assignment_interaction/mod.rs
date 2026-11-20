use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;

use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    staff_assignment_types::{StaffAssignmentArea, StaffAssignmentMode, StaffAssignmentTarget},
    staff_employment_types::Staff,
};

/// Assigns the selected worker to the habitat clicked in the world.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assign_selected_staff_to_habitat_under_world_pointer(
    mut pointer_inputs: MessageReader<PointerInput>,
    pointer_capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    world_pointer_ray: Res<WorldPointerRay>,
    spatial_query: SpatialQuery,
    habitat_index: Res<HabitatIndex>,
    topology_grid: Res<TopologyGrid>,
    assignment_modes: Query<(), (With<UiDocumentRoot>, With<StaffAssignmentMode>)>,
    selected_entity: WorldUiSelection,
    staff_assignments: Query<Option<&StaffAssignmentArea>, With<Staff>>,
    mut commands: Commands,
) {
    let primary_just_pressed = pointer_inputs.read().any(|input| {
        input.pointer_id.is_mouse() && input.button_just_pressed(PointerButton::Primary)
    });
    pointer_inputs.clear();
    if !primary_just_pressed
        || pointer_capture.over_ui
        || modal_input.0.is_some()
        || assignment_modes.is_empty()
    {
        return;
    }
    let Some(world_pointer_ray) = world_pointer_ray.0 else {
        return;
    };
    let Some(selected_worker) = selected_entity.subject() else {
        return;
    };
    let Ok(area) = staff_assignments.get(selected_worker) else {
        return;
    };
    let Some(world_hit) = spatial_query.cast_ray(
        world_pointer_ray.origin,
        world_pointer_ray.direction,
        f32::MAX,
        false,
        &SpatialQueryFilter::DEFAULT,
    ) else {
        return;
    };
    let world_position =
        world_pointer_ray.origin + *world_pointer_ray.direction * world_hit.distance;
    let Some(habitat) =
        locate_habitat_at_world_position(&habitat_index, world_position.xz(), *topology_grid)
    else {
        return;
    };

    let mut worker = commands.entity(selected_worker);
    if area.is_none_or(|area| area.0 != habitat) {
        worker.insert(StaffAssignmentArea(habitat));
    }
    worker.remove::<StaffAssignmentTarget>();
}

/// Cancel leaves the staff-assignment tool without mutating any staff relationship.
pub(crate) fn leave_staff_assignment_tools_after_cancel_action(
    mut commands: Commands,
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    assignment_tool_roots: Query<Entity, (With<UiDocumentRoot>, With<StaffAssignmentMode>)>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    if action_requests
        .read()
        .any(|request| request.action == GameAction::Cancel)
    {
        for assignment_tool_root in &assignment_tool_roots {
            commands
                .entity(assignment_tool_root)
                .remove::<StaffAssignmentMode>();
        }
    }
}

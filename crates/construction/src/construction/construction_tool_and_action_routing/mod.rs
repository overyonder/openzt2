use bevy::prelude::*;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;

use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteHoverTarget;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::SelectConstructionTool;

pub(super) fn construction_tool_active(tool: Res<ConstructionTool>) -> bool {
    *tool != ConstructionTool::Inspect
}

pub(super) fn select_construction_tool(
    mut selections: MessageReader<SelectConstructionTool>,
    mut tool: ResMut<ConstructionTool>,
    previews: Query<Entity, With<ConstructionPreview>>,
    relocating: Query<
        &openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference,
        With<openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject>,
    >,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    for selection in selections.read() {
        if *tool == ConstructionTool::Delete && selection.0 == ConstructionTool::Delete {
            cancel.write(CancelConstruction);
            *tool = ConstructionTool::Inspect;
            continue;
        }
        if *tool != selection.0 {
            if !previews.is_empty()
                || relocating
                    .iter()
                    .any(|object| selection.0 != ConstructionTool::Place(object.definition))
            {
                cancel.write(CancelConstruction);
            }
            *tool = selection.0;
        }
    }
}

pub(super) fn route_construction_actions(
    mut actions: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    ui_capture: Res<UiPointerCapture>,
    mut tool: ResMut<ConstructionTool>,
    previews: Query<Entity, With<ConstructionPreview>>,
    delete_target: Query<&DeleteHoverTarget, With<ConstructionCursor>>,
    mut commit: MessageWriter<CommitConstruction>,
    mut delete: MessageWriter<DeleteEntity>,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    for request in actions.read() {
        if modal_input.0.is_some() {
            continue;
        }
        if request.action == GameAction::Confirm
            && ui_capture.over_ui
            && matches!(
                request.source,
                openzt2_interface_state::input::input_types::ActionSource::Controller(_)
            )
        {
            continue;
        }
        match request.action {
            GameAction::Confirm => {
                if matches!(
                    *tool,
                    ConstructionTool::Terrain(_) | ConstructionTool::TankEdit
                ) {
                    continue;
                }
                if let Ok(preview) = previews.single() {
                    commit.write(CommitConstruction { preview });
                } else if *tool == ConstructionTool::Delete {
                    if let Ok(target) = delete_target.single() {
                        delete.write(DeleteEntity(target.0));
                    }
                }
            }
            GameAction::Cancel if *tool != ConstructionTool::Inspect || !previews.is_empty() => {
                cancel.write(CancelConstruction);
                *tool = ConstructionTool::Inspect;
            }
            _ => {}
        }
    }
}

/// Commits the active single-object construction preview on the primary
/// pointer press. Topology owns fence/path click sequences and elevated-path drags. UI
/// controls retain the press when Bevy marks any control as pressed, so
/// choosing a catalogue row or toolbar action cannot also place an object.
pub(super) fn route_pointer_construction_action(
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    ui_capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    previews: Query<(Entity, &ConstructionPreview)>,
    delete_target: Query<&DeleteHoverTarget, With<ConstructionCursor>>,
    mut commits: MessageWriter<CommitConstruction>,
    mut deletions: MessageWriter<DeleteEntity>,
) {
    if !primary_pointer.just_pressed || ui_capture.over_ui {
        return;
    }
    if *tool == ConstructionTool::Delete {
        if let Ok(target) = delete_target.single() {
            deletions.write(DeleteEntity(target.0));
        }
        return;
    }
    if !matches!(*tool, ConstructionTool::Place(_)) {
        return;
    }
    let Ok((preview, state)) = previews.single() else {
        return;
    };
    if matches!(state.validity, PlacementValidity::Valid { .. }) {
        commits.write(CommitConstruction { preview });
    }
}

pub(crate) fn cancel_preview(
    mut commands: Commands,
    mut cancel: MessageReader<CancelConstruction>,
    previews: Query<Entity, With<ConstructionPreview>>,
) {
    if cancel.read().next().is_none() {
        return;
    }
    for preview in &previews {
        commands.entity(preview).despawn();
    }
}

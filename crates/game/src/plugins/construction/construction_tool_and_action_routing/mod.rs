use bevy::prelude::*;

use crate::{
    plugins::input::input_types::{ActionRequest, GameAction},
    plugins::ui::picking::UiPointerCapture,
};

use super::{
    construction_interaction_types::{
        CancelConstruction, CommitConstruction, ConstructionCursor, ConstructionPreview,
        DeleteEntity, DeleteHoverTarget, PlacementValidity,
    },
    construction_tool_and_placement_policy_types::{ConstructionTool, SelectConstructionTool},
};

pub(super) fn construction_tool_active(tool: Res<ConstructionTool>) -> bool {
    *tool != ConstructionTool::Inspect
}

pub(super) fn select_construction_tool(
    mut selections: MessageReader<SelectConstructionTool>,
    mut tool: ResMut<ConstructionTool>,
    previews: Query<Entity, With<ConstructionPreview>>,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    for selection in selections.read() {
        if *tool == ConstructionTool::Delete && selection.0 == ConstructionTool::Delete {
            cancel.write(CancelConstruction);
            *tool = ConstructionTool::Inspect;
            continue;
        }
        if *tool != selection.0 {
            if !previews.is_empty() {
                cancel.write(CancelConstruction);
            }
            *tool = selection.0;
        }
    }
}

pub(super) fn route_construction_actions(
    mut actions: MessageReader<ActionRequest>,
    modal_input: Res<crate::plugins::ui::active_authored_ui_context::AuthoredModalInputCapture>,
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
                crate::plugins::input::input_types::ActionSource::Controller(_)
            )
        {
            continue;
        }
        match request.action {
            GameAction::Confirm => {
                if matches!(*tool, ConstructionTool::Terrain(_)) {
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
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
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

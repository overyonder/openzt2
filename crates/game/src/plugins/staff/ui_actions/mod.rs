use super::{
    staff_assignment_types::{
        KeeperAssignmentSelection, KeeperAssignmentTarget, SetStaffWorkerDutyAssignment,
        StaffAssignment, StaffAssignmentMode, TrainerAssignmentSelection,
    },
    staff_employment_types::Staff,
    staff_lifecycle_messages::FireStaffRequest,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiStaffActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::entity_selection_types::{InformationEntitySource, SelectedEntity},
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::staff_management::UiStaffAction;

pub(super) fn route_staff_assignment_selection_and_firing_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiStaffActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedEntity>,
    staff: Query<(), (With<Staff>, With<StaffAssignment>)>,
    mut firing: MessageWriter<FireStaffRequest>,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        for record in range.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiStaffAction::SetKeeperAssignmentSelection { assignment_index } => {
                    if let Some(target) = selected.0 {
                        commands.entity(target).insert(KeeperAssignmentSelection(
                            u32::try_from(*assignment_index).ok(),
                        ));
                    }
                }
                UiStaffAction::SetTrainerAssignmentSelection { assignment_index } => {
                    if let Some(target) = selected.0 {
                        commands.entity(target).insert(TrainerAssignmentSelection(
                            u32::try_from(*assignment_index).ok(),
                        ));
                    }
                }
                UiStaffAction::FireSelectedStaff => {
                    if let Some(entity) = selected.0.filter(|entity| staff.get(*entity).is_ok()) {
                        firing.write(FireStaffRequest { staff: entity });
                    }
                }
                _ => {}
            }
        }
    }
}

pub(super) fn route_staff_assignment_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiStaffActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedEntity>,
    sources: Query<&InformationEntitySource>,
    parents: Query<&ChildOf>,
    staff: Query<(), With<Staff>>,
    assignments: Query<(Entity, &StaffAssignment), With<Staff>>,
    mut worker_duty_assignments: MessageWriter<SetStaffWorkerDutyAssignment>,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiStaffAction::MarkSelectedEntityAsKeeperAssignmentTarget => {
                    if let Some(target) = selected.0 {
                        commands.entity(target).insert(KeeperAssignmentTarget);
                    }
                }
                UiStaffAction::ClearSelectedKeeperAssignmentTarget => {
                    if let Some(target) = selected.0 {
                        commands.entity(target).remove::<KeeperAssignmentTarget>();
                        for (worker, assignment) in &assignments {
                            if assignment.target == Some(target) {
                                commands.entity(worker).insert(StaffAssignment {
                                    target: None,
                                    ..*assignment
                                });
                            }
                        }
                    }
                }
                UiStaffAction::SetWorkerDutyAssignment { duty, assigned } => {
                    let worker =
                        information_source(activation.node, &sources, &parents).or(selected.0);
                    if let Some(worker) = worker.filter(|worker| staff.get(*worker).is_ok()) {
                        worker_duty_assignments.write(SetStaffWorkerDutyAssignment {
                            worker,
                            duty: *duty,
                            assigned: *assigned,
                        });
                    }
                }
                UiStaffAction::EnterStaffAssignmentMode => {
                    commands.entity(owner.0).insert(StaffAssignmentMode);
                }
                UiStaffAction::SetKeeperAssignmentSelection { .. }
                | UiStaffAction::SetTrainerAssignmentSelection { .. }
                | UiStaffAction::FireSelectedStaff => {}
            }
        }
    }
}

fn information_source(
    mut entity: Entity,
    sources: &Query<&InformationEntitySource>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    for _ in 0..16 {
        if let Ok(source) = sources.get(entity) {
            return Some(source.0);
        }
        entity = parents.get(entity).ok()?.parent();
    }
    None
}

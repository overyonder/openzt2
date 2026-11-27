use super::staff_assignment_interaction::RemoveSelectedStaffAssignment;
use super::{
    staff_assignment_types::{
        DisabledTrainerTricks, KeeperAssignmentSelection, KeeperAssignmentTarget,
        SetStaffWorkerDutyAssignment, StaffAssignedToTarget, StaffAssignmentMode,
        StaffAssignmentRecord, StaffAssignmentTarget, TrainerAssignmentSelection,
    },
    staff_employment_types::{Staff, StaffRole},
    staff_lifecycle_messages::FireStaffRequest,
};
use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::ui_document::action::staff_management::UiStaffAction;
use openzt2_game_data::world_definitions::staff_management::StaffRoleKind;
use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiStaffActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_zoo_state::{
    animal_lifecycle::types::SpeciesHandle,
    behavior_task_execution_types::running_actions::TrainerEnabledTricks,
};

pub(super) fn route_staff_assignment_selection_and_firing_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiStaffActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: WorldUiSelection,
    staff: Query<(), With<Staff>>,
    mut keepers: Query<&mut KeeperAssignmentSelection>,
    mut trainers: Query<&mut TrainerAssignmentSelection>,
    mut firing: MessageWriter<FireStaffRequest>,
    roles: Query<&StaffRole, With<Staff>>,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    mut removals: MessageWriter<RemoveSelectedStaffAssignment>,
    mut disabled_tricks: Query<&mut DisabledTrainerTricks>,
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
                    if let Some(target) = selected.subject() {
                        let selection =
                            KeeperAssignmentSelection(u32::try_from(*assignment_index).ok());
                        if let Ok(mut existing) = keepers.get_mut(target) {
                            existing.set_if_neq(selection);
                        } else {
                            commands.entity(target).insert(selection);
                        }
                    }
                }
                UiStaffAction::SetTrainerAssignmentSelection { assignment_index } => {
                    if let Some(target) = selected.subject() {
                        let selection =
                            TrainerAssignmentSelection(u32::try_from(*assignment_index).ok());
                        if let Ok(mut existing) = trainers.get_mut(target) {
                            existing.set_if_neq(selection);
                        } else {
                            commands.entity(target).insert(selection);
                        }
                    }
                }
                UiStaffAction::ClearSelectedKeeperAssignmentTarget
                | UiStaffAction::RemoveSelectedStaffAssignment => {
                    let Some(worker) = selected.subject() else {
                        continue;
                    };
                    let role = roles
                        .get(worker)
                        .ok()
                        .and_then(|role| definitions.get(&world_assets)?.find_staff(role.0));
                    if let Some(role) = role.filter(|role| {
                        matches!(role.role, StaffRoleKind::Keeper | StaffRoleKind::Trainer)
                    }) {
                        removals.write(RemoveSelectedStaffAssignment {
                            worker,
                            trainer: role.role == StaffRoleKind::Trainer,
                        });
                    }
                }
                UiStaffAction::SetTrainerTrickEnabled { trick, enabled } => {
                    let Some(trainer) = selected.subject() else {
                        continue;
                    };
                    if roles
                        .get(trainer)
                        .ok()
                        .and_then(|role| definitions.get(&world_assets)?.find_staff(role.0))
                        .is_some_and(|role| role.role == StaffRoleKind::Trainer)
                    {
                        if let Ok(mut disabled) = disabled_tricks.get_mut(trainer) {
                            if disabled.is_disabled(*trick) == *enabled {
                                disabled.set_disabled(*trick, !enabled);
                            }
                        }
                    }
                }
                UiStaffAction::FireSelectedStaff => {
                    if let Some(entity) = selected
                        .subject()
                        .filter(|entity| staff.get(*entity).is_ok())
                    {
                        firing.write(FireStaffRequest { staff: entity });
                    }
                }
                _ => {}
            }
        }
    }
}

pub(super) fn project_trainer_enabled_tricks(
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut trainers: Query<
        (
            Entity,
            &StaffRole,
            Option<&Children>,
            &DisabledTrainerTricks,
            Option<&mut TrainerEnabledTricks>,
        ),
        With<Staff>,
    >,
    rows: Query<&StaffAssignmentTarget, With<StaffAssignmentRecord>>,
    animals: Query<&SpeciesHandle>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (trainer, role, children, disabled, enabled) in &mut trainers {
        if !definitions
            .find_staff(role.0)
            .is_some_and(|role| role.role == StaffRoleKind::Trainer)
        {
            continue;
        }
        let mut assigned = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|row| rows.get(row).ok())
            .peekable();
        if assigned.peek().is_none() {
            if enabled.is_some() {
                commands.entity(trainer).remove::<TrainerEnabledTricks>();
            }
            continue;
        }
        let species: bevy::platform::collections::HashSet<_> = assigned
            .filter_map(|target| animals.get(target.0).ok().map(|species| species.species))
            .collect();
        let next = TrainerEnabledTricks(
            definitions
                .tricks()
                .filter(|trick| {
                    !disabled.is_disabled(trick.id)
                        && trick
                            .species
                            .iter()
                            .any(|species_id| species.contains(species_id))
                        && trick.levels.iter().any(|level| level.staff_can_train)
                })
                .map(|trick| trick.id)
                .collect(),
        );
        if let Some(mut enabled) = enabled {
            enabled.set_if_neq(next);
        } else {
            commands.entity(trainer).insert(next);
        }
    }
}

pub(super) fn route_staff_assignment_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiStaffActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: WorldUiSelection,
    sources: Query<&InformationEntitySource>,
    parents: Query<&ChildOf>,
    staff: Query<(), With<Staff>>,
    assignments: Query<&StaffAssignedToTarget>,
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
                    if let Some(target) = selected.subject() {
                        commands.entity(target).insert(KeeperAssignmentTarget);
                    }
                }
                UiStaffAction::ClearSelectedKeeperAssignmentTarget => {
                    if let Some(target) =
                        selected.subject().filter(|&target| !staff.contains(target))
                    {
                        commands.entity(target).remove::<KeeperAssignmentTarget>();

                        if let Ok(workers) = assignments.get(target) {
                            for worker in workers.iter() {
                                commands.entity(worker).remove::<StaffAssignmentTarget>();
                            }
                        }
                    }
                }
                UiStaffAction::SetWorkerDutyAssignment { task, assigned } => {
                    let worker = information_source(activation.node, &sources, &parents)
                        .or(selected.subject());
                    if let Some(worker) = worker.filter(|worker| staff.get(*worker).is_ok()) {
                        worker_duty_assignments.write(SetStaffWorkerDutyAssignment {
                            worker,
                            task: *task,
                            assigned: *assigned,
                        });
                    }
                }
                UiStaffAction::EnterStaffAssignmentMode => {
                    commands.entity(owner.0).insert(StaffAssignmentMode);
                }
                UiStaffAction::SetKeeperAssignmentSelection { .. }
                | UiStaffAction::SetTrainerAssignmentSelection { .. }
                | UiStaffAction::RemoveSelectedStaffAssignment
                | UiStaffAction::SetTrainerTrickEnabled { .. }
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

use bevy::prelude::*;

use crate::plugins::persistence::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceOperation,
    },
    save_slot_types::WorldSnapshotSavedToSlot,
};

use super::shell_navigation_request_types::{ExitApplication, ReturnToMainMenu};
use crate::plugins::ui::save_dialog_cancellation::SaveDialogCancellationRequests;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ReturnToMainMenuAfterSave;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ExitApplicationAfterSave;

/// Completes pending navigation only after persistence confirms the save.
pub(super) fn complete_or_cancel_pending_navigation_after_save_result(
    mut commands: Commands,
    mut cancellations: SaveDialogCancellationRequests,
    mut completed: MessageReader<WorldSnapshotSavedToSlot>,
    mut failed: MessageReader<WorldSnapshotPersistenceFailed>,
    return_to_menu: Query<Entity, With<ReturnToMainMenuAfterSave>>,
    exit_after_save: Query<Entity, With<ExitApplicationAfterSave>>,
    mut return_to_main_menu: MessageWriter<ReturnToMainMenu>,
    mut exit_application: MessageWriter<ExitApplication>,
) {
    let cancelled_owners = cancellations.read_cancelled_lifecycle_owners();
    for entity in &return_to_menu {
        if cancelled_owners.contains(&entity) {
            commands
                .entity(entity)
                .remove::<ReturnToMainMenuAfterSave>();
        }
    }
    for entity in &exit_after_save {
        if cancelled_owners.contains(&entity) {
            commands.entity(entity).remove::<ExitApplicationAfterSave>();
        }
    }
    if completed.read().next().is_some() {
        if return_to_menu
            .iter()
            .any(|entity| !cancelled_owners.contains(&entity))
        {
            return_to_main_menu.write(ReturnToMainMenu);
        }
        if exit_after_save
            .iter()
            .any(|entity| !cancelled_owners.contains(&entity))
        {
            exit_application.write(ExitApplication);
        }
        remove_pending_navigation_components(&mut commands, &return_to_menu, &exit_after_save);
    }

    if failed.read().any(|failure| {
        failure.persistence_operation == WorldSnapshotPersistenceOperation::SaveToSlot
    }) {
        remove_pending_navigation_components(&mut commands, &return_to_menu, &exit_after_save);
    }
}

fn remove_pending_navigation_components(
    commands: &mut Commands,
    return_to_menu: &Query<Entity, With<ReturnToMainMenuAfterSave>>,
    exit_after_save: &Query<Entity, With<ExitApplicationAfterSave>>,
) {
    for entity in return_to_menu {
        commands
            .entity(entity)
            .remove::<ReturnToMainMenuAfterSave>();
    }
    for entity in exit_after_save {
        commands.entity(entity).remove::<ExitApplicationAfterSave>();
    }
}

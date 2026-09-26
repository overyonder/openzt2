use bevy::prelude::*;
use openzt2_game_data::ui_document::{action::shell_navigation::UiShellAction, document::*};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        photos::photo_capture_types::CaptureScreenshotRequest,
        simulation_time::simulation_control_types::{SetSimulationPaused, SimulationControl},
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            ui_document_lifecycle_contracts::ShowUiRole,
        },
    },
};

use super::{
    exit_confirmation_presentation::ExitConfirmationPending,
    in_game_options_overlay_lifecycle::InGameOptionsOverlay,
    post_save_navigation::{ExitApplicationAfterSave, ReturnToMainMenuAfterSave},
    shell_navigation_request_types::ReturnToMainMenu,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShellActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Consumes only gameplay-shell outcomes. Menu navigation remains in
/// `route_authored_shell_ui_actions_to_navigation_requests`; persistence
/// remains owned by persistence.
pub(super) fn route_authored_in_game_shell_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiShellActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    parents: Query<&ChildOf>,
    pending: Query<(), With<ExitConfirmationPending>>,
    mut return_to_main_menu: MessageWriter<ReturnToMainMenu>,
    mut screenshots: MessageWriter<CaptureScreenshotRequest>,
    mut show: MessageWriter<ShowUiRole>,
    simulation: Option<Res<SimulationControl>>,
    overlays: Query<(), With<InGameOptionsOverlay>>,
    mut set_paused: MessageWriter<SetSimulationPaused>,
    cameras: Query<(Entity, &Camera), With<Camera3d>>,
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
                UiShellAction::DismissExitConfirmationDialog => {
                    commands.entity(owner.0).remove::<ExitConfirmationPending>();
                    if let Ok(parent) = parents.get(owner.0) {
                        if pending.get(parent.parent()).is_ok() {
                            commands.entity(parent.parent()).despawn();
                        }
                    }
                }
                UiShellAction::MarkExitConfirmationPending => {
                    commands.entity(owner.0).insert(ExitConfirmationPending);
                }
                UiShellAction::ReturnToMainMenu => {
                    return_to_main_menu.write(ReturnToMainMenu);
                }
                UiShellAction::ShowExitConfirmationDialog => {
                    let confirmation = commands
                        .spawn((ExitConfirmationPending, Visibility::Inherited))
                        .id();
                    show.write(ShowUiRole {
                        role: UiDocumentRole::Modal,
                        owner: confirmation,
                    });
                }
                UiShellAction::ShowInGameOptionsOverlay => {
                    if !overlays.is_empty() {
                        continue;
                    }
                    let Some(simulation) = simulation.as_deref() else {
                        continue;
                    };
                    let options = commands
                        .spawn((
                            InGameOptionsOverlay::remembering_previous_pause_state(
                                simulation.paused,
                            ),
                            Visibility::Inherited,
                        ))
                        .id();
                    set_paused.write(SetSimulationPaused(true));
                    show.write(ShowUiRole {
                        role: UiDocumentRole::InGameOptions,
                        owner: options,
                    });
                }
                UiShellAction::ReturnToMainMenuAfterWorldSnapshotSave => {
                    commands.entity(owner.0).insert(ReturnToMainMenuAfterSave);
                }
                UiShellAction::ExitApplicationAfterWorldSnapshotSave => {
                    commands.entity(owner.0).insert(ExitApplicationAfterSave);
                }
                UiShellAction::CaptureScreenshotFromSoleActive3dCamera => {
                    let mut active = cameras
                        .iter()
                        .filter_map(|(entity, camera)| camera.is_active.then_some(entity));
                    if let Some(camera) = active.next().filter(|_| active.next().is_none()) {
                        screenshots.write(CaptureScreenshotRequest { camera });
                    }
                }
                UiShellAction::FinishSplashPresentation
                | UiShellAction::NavigateBackFromDownloads
                | UiShellAction::NavigateBackFromOptions
                | UiShellAction::NavigateBackFromMapSelection
                | UiShellAction::SelectFreeformModeOrStartSelectedWorld
                | UiShellAction::SelectChallengeModeOrStartSelectedWorld
                | UiShellAction::SelectCampaignModeOrStartSelectedWorld
                | UiShellAction::StartSelectedWorld
                | UiShellAction::ShowOptions
                | UiShellAction::ShowDownloads
                | UiShellAction::ShowSavedGames
                | UiShellAction::FilterWorldChoicesToLocation { .. }
                | UiShellAction::FilterWorldChoicesToExpansionPack { .. }
                | UiShellAction::ShowSecondaryGlobeBiome { .. }
                | UiShellAction::ExitApplication => {}
            }
        }
    }
}

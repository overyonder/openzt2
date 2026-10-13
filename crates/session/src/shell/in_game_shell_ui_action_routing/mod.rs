use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::{action::shell_navigation::UiShellAction, document::*};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_interface_state::information::entity_selection_types::SelectionRequest;
use openzt2_interface_state::photos::photo_capture_types::CaptureScreenshotRequest;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::GameViewDocumentOwner;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPauseRequest;

use super::{
    exit_confirmation_presentation::ExitConfirmationPending,
    in_game_options_overlay_lifecycle::InGameOptionsOverlay,
    post_save_navigation::{ExitApplicationAfterSave, ReturnToMainMenuAfterSave},
    shell_navigation_request_types::{ExitApplication, ReturnToMainMenu},
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShellActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct OverheadInformationDismissalTargets<'w> {
    selected_entity: Res<'w, SelectedEntity>,
    selected_catalogue_entry: ResMut<'w, SelectedCatalogueEntry>,
    selection_requests: MessageWriter<'w, SelectionRequest>,
}

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
    (mut return_to_main_menu, mut exit_application): (
        MessageWriter<ReturnToMainMenu>,
        MessageWriter<ExitApplication>,
    ),
    mut screenshots: MessageWriter<CaptureScreenshotRequest>,
    mut show: MessageWriter<ShowUiRole>,
    simulation: Option<Res<SimulationControl>>,
    overlays: Query<(), With<InGameOptionsOverlay>>,
    mut set_paused: MessageWriter<SimulationPauseRequest>,
    cameras: Query<(Entity, &Camera), With<Camera3d>>,
    game_view: GameViewDocumentOwner,
    mut information: OverheadInformationDismissalTargets,
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
                UiShellAction::ExitApplication => {
                    exit_application.write(ExitApplication);
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
                UiShellAction::ShowInGameOptionsOverlay
                | UiShellAction::OpenInGameOptionsFromOverheadEscape => {
                    if record.action == UiShellAction::OpenInGameOptionsFromOverheadEscape {
                        if information.selected_entity.0.is_some() {
                            information.selection_requests.write(SelectionRequest {
                                entity: None,
                                source: activation.source,
                            });
                            continue;
                        }
                        if information.selected_catalogue_entry.0.is_some() {
                            information.selected_catalogue_entry.0 = None;
                            continue;
                        }
                    }
                    if !overlays.is_empty() {
                        continue;
                    }
                    let Some(simulation) = simulation.as_deref() else {
                        continue;
                    };
                    let overlay = commands
                        .spawn((
                            InGameOptionsOverlay::remembering_previous_pause_state(
                                simulation.paused,
                            ),
                            Visibility::Inherited,
                        ))
                        .id();
                    set_paused.write(SimulationPauseRequest::Set(true));
                    // The menu belongs with the game view, so the dialogs it
                    // opens as it closes, such as returning to the main menu or
                    // saving, outlive it.
                    show.write(ShowUiRole {
                        role: UiDocumentRole::InGameOptions,
                        owner: game_view.owner(overlay),
                    });
                }
                // Navigation waits on the save dialog's owner, where its save
                // completes or its cancellation abandons the navigation.
                UiShellAction::ReturnToMainMenuAfterWorldSnapshotSave => {
                    if let Ok(parent) = parents.get(owner.0) {
                        commands
                            .entity(game_view.owner(parent.parent()))
                            .insert(ReturnToMainMenuAfterSave);
                    }
                }
                UiShellAction::ExitApplicationAfterWorldSnapshotSave => {
                    if let Ok(parent) = parents.get(owner.0) {
                        commands
                            .entity(game_view.owner(parent.parent()))
                            .insert(ExitApplicationAfterSave);
                    }
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
                | UiShellAction::ShowSecondaryGlobeBiome { .. } => {}
            }
        }
    }
}

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::action::shell_navigation::UiShellAction;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    game_session_types::WorldSessionMode,
    plugins::{
        persistence::profile_types::SelectProfile,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

use super::{
    shell_navigation_request_types::{
        ExitApplication, NavigateShellBack, SelectWorldSessionMode, ShowDownloads, ShowOptions,
        ShowSavedGames, SplashFinished, StartSelectedWorld,
    },
    shell_selection_types::{ProfileChoice, ShellSelection, WorldChoice, WorldChoiceView},
    world_selection_presentation_types::{SecondaryGlobe, SelectedWorldCatalogueFilter},
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShellActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct ShellUiActionMessageWriters<'w> {
    select_profile: MessageWriter<'w, SelectProfile>,
    navigate_back: MessageWriter<'w, NavigateShellBack>,
    show_options: MessageWriter<'w, ShowOptions>,
    show_downloads: MessageWriter<'w, ShowDownloads>,
    show_saved_games: MessageWriter<'w, ShowSavedGames>,
    select_play_mode: MessageWriter<'w, SelectWorldSessionMode>,
    start_world: MessageWriter<'w, StartSelectedWorld>,
    finish_splash: MessageWriter<'w, SplashFinished>,
    exit_application: MessageWriter<'w, ExitApplication>,
}

pub(super) fn route_authored_shell_ui_actions_to_navigation_requests(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiShellActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    world_views: Query<&WorldChoiceView>,
    worlds: Query<&WorldChoice>,
    profiles: Query<&ProfileChoice>,
    parents: Query<&ChildOf>,
    selection: Res<ShellSelection>,
    mut messages: ShellUiActionMessageWriters,
) {
    for activation in activations.read() {
        if let Some(profile_choice) = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| profiles.get(entity).ok())
        {
            if matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit) {
                messages.select_profile.write(SelectProfile {
                    requested_profile_identifier: profile_choice.0,
                });
            }
            continue;
        }

        let Ok((action_range, document_owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(document_root) = roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        for action_record in action_range.authored_action_records(document) {
            if activation.trigger != action_record.trigger {
                continue;
            }
            match &action_record.action {
                UiShellAction::FinishSplashPresentation => {
                    messages.finish_splash.write(SplashFinished);
                }
                UiShellAction::NavigateBackFromDownloads
                | UiShellAction::NavigateBackFromOptions
                | UiShellAction::NavigateBackFromMapSelection => {
                    messages.navigate_back.write(NavigateShellBack);
                }
                UiShellAction::SelectFreeformModeOrStartSelectedWorld => {
                    route_play_mode_or_start_selected_world(
                        WorldSessionMode::Freeform,
                        activation.node,
                        &selection,
                        &world_views,
                        &worlds,
                        &mut messages.select_play_mode,
                        &mut messages.start_world,
                    );
                }
                UiShellAction::SelectChallengeModeOrStartSelectedWorld => {
                    route_play_mode_or_start_selected_world(
                        WorldSessionMode::Challenge,
                        activation.node,
                        &selection,
                        &world_views,
                        &worlds,
                        &mut messages.select_play_mode,
                        &mut messages.start_world,
                    );
                }
                UiShellAction::SelectCampaignModeOrStartSelectedWorld => {
                    route_play_mode_or_start_selected_world(
                        WorldSessionMode::Campaign,
                        activation.node,
                        &selection,
                        &world_views,
                        &worlds,
                        &mut messages.select_play_mode,
                        &mut messages.start_world,
                    );
                }
                UiShellAction::StartSelectedWorld => {
                    messages.start_world.write(StartSelectedWorld);
                }
                UiShellAction::ShowOptions => {
                    messages.show_options.write(ShowOptions);
                }
                UiShellAction::ShowDownloads => {
                    messages.show_downloads.write(ShowDownloads);
                }
                UiShellAction::ShowSavedGames => {
                    messages.show_saved_games.write(ShowSavedGames);
                }
                UiShellAction::ExitApplication => {
                    messages.exit_application.write(ExitApplication);
                }
                UiShellAction::FilterWorldChoicesToLocation { world_location } => {
                    let filter = if *world_location == openzt2_game_data::AssetId::default() {
                        SelectedWorldCatalogueFilter::All
                    } else {
                        SelectedWorldCatalogueFilter::BiomeLocationGroup(*world_location)
                    };
                    if let Ok(shell_screen_owner) = parents.get(document_owner.0) {
                        commands.entity(shell_screen_owner.parent()).insert(filter);
                    }
                }
                UiShellAction::FilterWorldChoicesToExpansionPack {
                    expansion_pack_identifier,
                } => {
                    if let Ok(shell_screen_owner) = parents.get(document_owner.0) {
                        commands.entity(shell_screen_owner.parent()).insert(
                            SelectedWorldCatalogueFilter::ExpansionPack(*expansion_pack_identifier),
                        );
                    }
                }
                UiShellAction::ShowSecondaryGlobeBiome { biome } => {
                    commands
                        .entity(document_owner.0)
                        .insert(SecondaryGlobe(*biome));
                }
                UiShellAction::ShowExitConfirmationDialog
                | UiShellAction::DismissExitConfirmationDialog
                | UiShellAction::MarkExitConfirmationPending
                | UiShellAction::ReturnToMainMenu
                | UiShellAction::ShowInGameOptionsOverlay
                | UiShellAction::ReturnToMainMenuAfterWorldSnapshotSave
                | UiShellAction::ExitApplicationAfterWorldSnapshotSave
                | UiShellAction::CaptureScreenshotFromSoleActive3dCamera => {}
            }
        }
    }
}

fn route_play_mode_or_start_selected_world(
    requested_play_mode: WorldSessionMode,
    activated_node: Entity,
    selection: &ShellSelection,
    world_choice_views: &Query<&WorldChoiceView>,
    world_choices: &Query<&WorldChoice>,
    select_play_mode_requests: &mut MessageWriter<SelectWorldSessionMode>,
    start_selected_world_requests: &mut MessageWriter<StartSelectedWorld>,
) {
    let activated_world_has_requested_mode = world_choice_views
        .get(activated_node)
        .ok()
        .and_then(|view| world_choices.get(view.0).ok())
        .is_some_and(|choice| choice.mode == requested_play_mode);
    if selection.mode == Some(requested_play_mode) && selection.scenario.is_some()
        || activated_world_has_requested_mode
    {
        start_selected_world_requests.write(StartSelectedWorld);
    } else {
        select_play_mode_requests.write(SelectWorldSessionMode(requested_play_mode));
    }
}

use bevy::{
    camera::CameraOutputMode, prelude::*, render::render_resource::BlendState,
    ui_render::UiAntiAlias,
};

use crate::{
    application_lifecycle::GamePhase,
    plugins::ui::{
        ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
        ui_document_lifecycle_contracts::UiRoleRequests,
    },
};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use super::{
    shell_navigation_request_types::SplashFinished,
    shell_screen_presentation_types::{ShellClearCamera, ShellUiCamera},
    shell_selection_types::ShellScreen,
};

pub(super) fn create_boot_splash_screen_and_shell_cameras(
    mut commands: Commands,
    shell_ui_cameras: Query<(), With<ShellUiCamera>>,
    shell_clear_cameras: Query<(), With<ShellClearCamera>>,
    mut requested_ui_roles: ResMut<UiRoleRequests>,
) {
    if shell_clear_cameras.is_empty() {
        commands.spawn((
            Camera2d,
            Camera {
                order: -100,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            ShellClearCamera,
        ));
    }
    if shell_ui_cameras.is_empty() {
        commands.spawn((
            Camera2d,
            Camera {
                order: 100,
                output_mode: CameraOutputMode::Write {
                    blend_state: Some(BlendState::ALPHA_BLENDING),
                    clear_color: ClearColorConfig::None,
                },
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            // Blue Fang's rectangular UI images are already authored at
            // pixel boundaries. Bevy's shape-edge antialiasing reduces
            // coverage at every quad boundary, which exposes the clear
            // colour between adjacent background tiles.
            UiAntiAlias::Off,
            IsDefaultUiCamera,
            ShellUiCamera,
        ));
    }
    let splash_screen_owner = commands
        .spawn((ShellScreen::Splash, Visibility::Inherited))
        .id();
    requested_ui_roles.request(UiDocumentRole::Splash, splash_screen_owner);
}

pub(super) fn enter_main_menu_after_splash_completion_or_ui_document_asset_load_failure(
    mut finished: MessageReader<SplashFinished>,
    mut document_asset_load_failed: MessageReader<UiDocumentAssetLoadFailed>,
    screens: Query<(Entity, &ShellScreen)>,
    mut next_phase: ResMut<NextState<GamePhase>>,
) {
    let splash_screen_owner = screens
        .iter()
        .find_map(|(entity, screen)| (*screen == ShellScreen::Splash).then_some(entity));
    let splash_finished = finished.read().next().is_some();
    let splash_unavailable = document_asset_load_failed
        .read()
        .any(|failure| Some(failure.owner) == splash_screen_owner);
    if splash_screen_owner.is_some() && (splash_finished || splash_unavailable) {
        next_phase.set(GamePhase::MainMenu);
    }
}

use bevy::{
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

use super::{
    display_settings_types::{
        DisplayMode, DisplaySettings, ReplaceDisplaySettingsRequest, SelectScreenResolutionRequest,
        SupportedDisplayResolutions,
    },
    display_settings_validation::validate_proposed_display_settings,
};

/// Refreshes the options-screen capability facts from Bevy's winit-owned
/// monitor entities. Duplicate refresh-rate/bit-depth modes intentionally
/// collapse to one selectable width/height pair, matching the source UI.
pub(super) fn validate_selected_screen_resolution_and_request_display_settings_replacement(
    mut requests: MessageReader<SelectScreenResolutionRequest>,
    current: Res<DisplaySettings>,
    supported: Res<SupportedDisplayResolutions>,
    mut display_requests: MessageWriter<ReplaceDisplaySettingsRequest>,
) {
    for request in requests.read() {
        if !supported.0.contains(&request.0) {
            continue;
        }
        display_requests.write(ReplaceDisplaySettingsRequest(DisplaySettings {
            width: request.0.width,
            height: request.0.height,
            ..*current
        }));
    }
}

pub(super) fn validate_and_accept_requested_display_settings(
    mut requests: MessageReader<ReplaceDisplaySettingsRequest>,
    mut current: ResMut<DisplaySettings>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
) {
    let primary_window_exists = primary_window.single().is_ok();
    for request in requests.read() {
        if validate_proposed_display_settings(request.0, primary_window_exists).is_err() {
            continue;
        }
        if *current != request.0 {
            *current = request.0;
        }
    }
}

pub(super) fn apply_accepted_display_settings_to_primary_window(
    settings: Res<DisplaySettings>,
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = primary_window.single_mut() else {
        return;
    };
    window
        .resolution
        .set_physical_resolution(settings.width, settings.height);
    window.mode = match settings.mode {
        DisplayMode::Windowed => WindowMode::Windowed,
        DisplayMode::BorderlessFullscreen => {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        }
    };
    window.present_mode = settings.pacing.bevy_present_mode();
}

mod audio_settings_application;
mod authored_audio_volume_controls;
mod authored_options_settings;
use openzt2_interface_state::settings::audio_settings_types;
mod display_resolution_ui_presentation;
mod display_settings_application;
use openzt2_interface_state::settings::display_settings_types;
mod display_settings_validation;
mod graphics_detail_setting_ui_presentation;
mod graphics_settings_application;
mod graphics_settings_presentation_types;
mod graphics_settings_validation;
use openzt2_interface_state::settings::online_message_policy_types;
use openzt2_interface_state::settings::options_screen_settings_draft_types;

use audio_settings_types::{AudioSettings, ReplaceAudioSettingsRequest};
use display_settings_types::{
    DisplaySettings, ReplaceDisplaySettingsRequest, SelectScreenResolutionRequest,
    SupportedDisplayResolutions,
};
use online_message_policy_types::OnlineMessagePolicy;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::graphics_settings_types::ReplaceGraphicsSettingsRequest;

use bevy::{ecs::schedule::common_conditions::*, prelude::*};

use openzt2_interface_state::persistence::profile_types::ProfileSelected;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiSliderValueChanged;
use openzt2_interface_state::ui::UiSet;
use openzt2_world_state::application_schedule::GameSet;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DisplaySettings>()
            .init_resource::<display_settings_application::AcceptedDisplaySettingsReady>()
            .add_message::<ProfileSelected>()
            .add_message::<UiSliderValueChanged>()
            .init_resource::<AudioSettings>()
            .init_resource::<GraphicsSettings>()
            .init_resource::<OnlineMessagePolicy>()
            .init_resource::<
                openzt2_interface_state::settings::modder_tools_settings_types::ModderToolsSettings,
            >()
            .init_resource::<SupportedDisplayResolutions>()
            .add_message::<ReplaceDisplaySettingsRequest>()
            .add_message::<SelectScreenResolutionRequest>()
            .add_message::<ReplaceGraphicsSettingsRequest>()
            .add_message::<ReplaceAudioSettingsRequest>()
            .add_systems(
                Update,
                (
                    authored_options_settings::apply_authored_options_setting_edits,
                    authored_options_settings::apply_authored_mouse_look_settings,
                )
                    .before(openzt2_interface_state::shell::ShellNavigationAfterSettings)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    authored_options_settings::initialize_projected_options_settings_drafts,
                    authored_options_settings::project_options_draft_into_radio_buttons
                        .after(authored_options_settings::initialize_projected_options_settings_drafts),
                    authored_options_settings::project_current_camera_mouse_look_into_controls,
                )
                    .in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                (
                    authored_audio_volume_controls::apply_authored_audio_volume_edits,
                    audio_settings_application::replace_accepted_audio_settings_from_requests,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                authored_audio_volume_controls::project_accepted_audio_volume_into_controls
                    .after(UiSet::Projection)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                (
                    display_resolution_ui_presentation::refresh_supported_display_resolutions,
                    display_settings_application::validate_selected_screen_resolution_and_request_display_settings_replacement
                        .run_if(on_message::<SelectScreenResolutionRequest>),
                    display_settings_application::validate_and_accept_requested_display_settings
                        .after(authored_options_settings::apply_authored_options_setting_edits)
                        .before(openzt2_interface_state::shell::ShellNavigationAfterSettings)
                        .run_if(on_message::<ReplaceDisplaySettingsRequest>),
                ).chain().in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                graphics_settings_application::validate_and_accept_requested_graphics_settings
                    .after(authored_options_settings::apply_authored_options_setting_edits)
                    .before(openzt2_interface_state::shell::ShellNavigationAfterSettings)
                    .run_if(on_message::<ReplaceGraphicsSettingsRequest>)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                display_settings_application::apply_ready_display_settings_to_primary_window
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    graphics_settings_application::project_multisample_count_to_zoo_cameras,
                    graphics_settings_application::project_shadow_detail_to_environment_lights,
                )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                graphics_settings_application::apply_accepted_ui_text_shadow_setting
                    .after(UiSet::Projection)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                graphics_detail_setting_ui_presentation::apply_accepted_graphics_detail_slider_edits_to_draft
                    .run_if(on_message::<UiSliderValueChanged>)
                    .before(authored_options_settings::apply_authored_options_setting_edits)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    display_resolution_ui_presentation::
                        set_authored_display_resolution_list_row_count,
                    display_resolution_ui_presentation::
                        project_supported_display_resolutions_into_authored_rows,
                    display_resolution_ui_presentation::
                        copy_activated_display_resolution_into_settings_draft
                            .after(display_resolution_ui_presentation::project_supported_display_resolutions_into_authored_rows)
                            .after(authored_options_settings::initialize_projected_options_settings_drafts),
                    display_resolution_ui_presentation::
                        project_drafted_display_resolution_selection_into_authored_rows
                            .after(display_resolution_ui_presentation::copy_activated_display_resolution_into_settings_draft),
                    display_resolution_ui_presentation::
                        project_drafted_display_resolution_into_authored_text
                            .after(display_resolution_ui_presentation::copy_activated_display_resolution_into_settings_draft),
                )
                    .in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                (
                    graphics_detail_setting_ui_presentation::
                        set_authored_graphics_detail_setting_list_row_count,
                    graphics_detail_setting_ui_presentation::
                        project_drafted_graphics_detail_settings_into_authored_rows
                            .after(authored_options_settings::initialize_projected_options_settings_drafts),
                )
                    .in_set(UiSet::DomainProjection),
            );
    }
}

#[cfg(test)]
mod display_and_graphics_settings_tests;

#[cfg(test)]
mod graphics_detail_setting_tests;

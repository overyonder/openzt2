pub(in crate::plugins) mod settings_action_types;
pub(in crate::plugins) mod settings_ui_routing_and_projection;

mod audio_settings_application;
pub mod audio_settings_types;
mod audio_settings_ui_routing_and_projection;
mod display_resolution_ui_presentation;
mod display_settings_application;
pub mod display_settings_types;
mod display_settings_validation;
mod graphics_settings_application;
pub mod graphics_settings_types;
mod graphics_settings_validation;
pub mod online_message_policy_types;
pub mod options_screen_settings_draft_types;

use audio_settings_types::{AudioSettings, ReplaceAudioSettingsRequest};
use display_settings_types::{
    DisplaySettings, ReplaceDisplaySettingsRequest, SelectScreenResolutionRequest,
    SupportedDisplayResolutions,
};
use graphics_settings_types::{GraphicsSettings, ReplaceGraphicsSettingsRequest};
use online_message_policy_types::OnlineMessagePolicy;
use options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft;

use bevy::{ecs::schedule::common_conditions::*, prelude::*};

use crate::{application_schedule::GameSet, plugins::ui::UiSet};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DisplaySettings>()
            .init_resource::<AudioSettings>()
            .init_resource::<GraphicsSettings>()
            .init_resource::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>()
            .init_resource::<OnlineMessagePolicy>()
            .init_resource::<SupportedDisplayResolutions>()
            .add_message::<ReplaceDisplaySettingsRequest>()
            .add_message::<SelectScreenResolutionRequest>()
            .add_message::<ReplaceGraphicsSettingsRequest>()
            .add_message::<ReplaceAudioSettingsRequest>()
            .add_systems(
                Update,
                settings_ui_routing_and_projection::project_settings_to_authored_controls
                    .after(UiSet::Projection)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                (
                    audio_settings_ui_routing_and_projection::
                        route_authored_audio_volume_actions_to_replacement_requests,
                    audio_settings_application::replace_accepted_audio_settings_from_requests,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                audio_settings_ui_routing_and_projection::
                    project_accepted_audio_volume_settings_into_authored_controls
                    .after(UiSet::Projection)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                display_resolution_ui_presentation::
                    synchronize_supported_display_resolutions_from_primary_monitor
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                display_settings_application::
                    validate_selected_screen_resolution_and_request_display_settings_replacement
                    .run_if(on_message::<SelectScreenResolutionRequest>)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                display_settings_application::validate_and_accept_requested_display_settings
                    .run_if(on_message::<ReplaceDisplaySettingsRequest>)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                graphics_settings_application::validate_and_accept_requested_graphics_settings
                    .run_if(on_message::<ReplaceGraphicsSettingsRequest>)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                display_settings_application::apply_accepted_display_settings_to_primary_window
                    .run_if(resource_changed::<DisplaySettings>)
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    graphics_settings_application::
                        apply_accepted_multisample_count_to_zoo_cameras,
                    graphics_settings_application::
                        apply_accepted_shadow_detail_to_environment_lights,
                )
                    .run_if(
                        graphics_settings_application::
                            graphics_settings_changed_or_render_target_was_added,
                    )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    display_resolution_ui_presentation::
                        set_authored_display_resolution_list_row_count,
                    display_resolution_ui_presentation::
                        project_supported_display_resolutions_into_authored_rows,
                    display_resolution_ui_presentation::
                        copy_activated_display_resolution_into_settings_draft,
                    display_resolution_ui_presentation::
                        project_drafted_display_resolution_selection_into_authored_rows,
                )
                    .chain()
                    .in_set(GameSet::Ui),
            );
    }
}

#[cfg(test)]
mod display_and_graphics_settings_tests;

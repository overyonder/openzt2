use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationSettingAction, UiInformationAction,
};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        camera::camera_runtime_state_types::CameraMouseLook,
        settings::{
            display_settings_types::DisplayMode, graphics_settings_types::GraphicsSettings,
        },
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            authored_ui_selection_state::UiSelected,
        },
    },
};

use super::settings_action_types::SettingsActionTargets;
use crate::plugins::ui::authored_ui_action_projection_components::UiInformationActions;

/// Displays the options draft in radio buttons and checkboxes.
pub(in crate::plugins) fn project_settings_to_authored_controls(
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    document_roots: Query<&UiDocumentRoot>,
    draft_settings: Res<crate::plugins::settings::options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    zoo_cameras: Query<
        Has<CameraMouseLook>,
        With<crate::plugins::camera::camera_runtime_state_types::ZooCamera>,
    >,
    mut authored_controls: Query<(Ref<UiInformationActions>, &UiDocumentOwner, &mut UiSelected)>,
) {
    let free_mouse_look_enabled = zoo_cameras.iter().next().unwrap_or(true);
    for (authored_actions, document_owner, mut selected) in &mut authored_controls {
        if !draft_settings.is_changed() && !authored_actions.is_added() {
            continue;
        }
        let Ok(document_root) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document_asset) = ui_document_assets.get(&document_root.document) else {
            continue;
        };
        let mut authored_action_records =
            authored_actions.authored_action_records(ui_document_asset);
        let authored_setting = authored_action_records.find_map(|record| match &record.action {
            UiInformationAction::ApplySetting { setting } => Some(setting),
            _ => None,
        });
        let Some(authored_setting) = authored_setting else {
            continue;
        };
        let should_be_selected = information_setting_matches_current_draft(
            authored_setting,
            &draft_settings,
            free_mouse_look_enabled,
        );
        if selected.0 != should_be_selected {
            selected.0 = should_be_selected;
        }
    }
}

fn information_setting_matches_current_draft(
    authored_setting: &InformationSettingAction,
    draft_settings: &crate::plugins::settings::options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
    free_mouse_look_enabled: bool,
) -> bool {
    match authored_setting {
        InformationSettingAction::HighestDetail => {
            draft_settings.graphics == GraphicsSettings::from_source_highest_detail_preset()
        }
        InformationSettingAction::HighDetail => {
            draft_settings.graphics == GraphicsSettings::from_source_high_detail_preset()
        }
        InformationSettingAction::MediumDetail => {
            draft_settings.graphics == GraphicsSettings::from_source_medium_detail_preset()
        }
        InformationSettingAction::CustomDetail => {
            draft_settings.graphics != GraphicsSettings::from_source_highest_detail_preset()
                && draft_settings.graphics != GraphicsSettings::from_source_high_detail_preset()
                && draft_settings.graphics != GraphicsSettings::from_source_medium_detail_preset()
        }
        InformationSettingAction::Fullscreen => {
            draft_settings.display.mode == DisplayMode::BorderlessFullscreen
        }
        InformationSettingAction::Windowed => draft_settings.display.mode == DisplayMode::Windowed,
        InformationSettingAction::FreeMouseLook(enabled) => free_mouse_look_enabled == *enabled,
        InformationSettingAction::MessageOfTheDay(enabled) => {
            draft_settings.message_of_the_day == *enabled
        }
        InformationSettingAction::Refresh
        | InformationSettingAction::Accept
        | InformationSettingAction::Back => false,
    }
}

pub(in crate::plugins) fn apply_authored_setting_action(
    authored_setting: &InformationSettingAction,
    targets: &mut SettingsActionTargets,
    commands: &mut Commands,
) {
    match authored_setting {
        InformationSettingAction::Refresh | InformationSettingAction::Back => {
            targets.draft_settings.display = *targets.accepted_display_settings;
            targets.draft_settings.graphics = *targets.accepted_graphics_settings;
            targets.draft_settings.message_of_the_day = targets.online_message_policy.enabled;
            targets.draft_settings.dirty_display = false;
            targets.draft_settings.dirty_graphics = false;
        }
        InformationSettingAction::HighestDetail => {
            targets.draft_settings.graphics = GraphicsSettings::from_source_highest_detail_preset();
            targets.draft_settings.dirty_graphics = true;
        }
        InformationSettingAction::HighDetail => {
            targets.draft_settings.graphics = GraphicsSettings::from_source_high_detail_preset();
            targets.draft_settings.dirty_graphics = true;
        }
        InformationSettingAction::MediumDetail => {
            targets.draft_settings.graphics = GraphicsSettings::from_source_medium_detail_preset();
            targets.draft_settings.dirty_graphics = true;
        }
        InformationSettingAction::CustomDetail => {}
        InformationSettingAction::Fullscreen => {
            targets.draft_settings.display.mode = DisplayMode::BorderlessFullscreen;
            targets.draft_settings.dirty_display = true;
        }
        InformationSettingAction::Windowed => {
            targets.draft_settings.display.mode = DisplayMode::Windowed;
            targets.draft_settings.dirty_display = true;
        }
        InformationSettingAction::FreeMouseLook(enabled) => {
            targets.zoo_cameras.iter().for_each(|zoo_camera| {
                if *enabled {
                    commands.entity(zoo_camera).insert(CameraMouseLook);
                } else {
                    commands.entity(zoo_camera).remove::<CameraMouseLook>();
                }
            });
        }
        InformationSettingAction::MessageOfTheDay(enabled) => {
            targets.draft_settings.message_of_the_day = *enabled;
        }
        InformationSettingAction::Accept => {
            if targets.draft_settings.dirty_display {
                targets.display_setting_requests.write(
                    crate::plugins::settings::display_settings_types::ReplaceDisplaySettingsRequest(
                        targets.draft_settings.display,
                    ),
                );
            }
            if targets.draft_settings.dirty_graphics {
                targets
                    .graphics_setting_requests
                    .write(crate::plugins::settings::graphics_settings_types::ReplaceGraphicsSettingsRequest(
                        targets.draft_settings.graphics,
                    ));
            }
            targets.online_message_policy.enabled = targets.draft_settings.message_of_the_day;
            targets.draft_settings.dirty_display = false;
            targets.draft_settings.dirty_graphics = false;
        }
    }
}

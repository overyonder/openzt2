use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::graphics_settings_types::ReplaceGraphicsSettingsRequest;

use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::settings::display_settings_types::DisplaySettings;
use openzt2_interface_state::settings::display_settings_types::ReplaceDisplaySettingsRequest;
use openzt2_interface_state::settings::online_message_policy_types::OnlineMessagePolicy;
use openzt2_interface_state::settings::options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft;

#[derive(SystemParam)]
pub struct SettingsActionTargets<'w, 's> {
    pub(crate) draft_settings: ResMut<'w, OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    pub(crate) accepted_display_settings: Res<'w, DisplaySettings>,
    pub(crate) accepted_graphics_settings: Res<'w, GraphicsSettings>,
    pub(crate) online_message_policy: ResMut<'w, OnlineMessagePolicy>,
    pub(crate) display_setting_requests: MessageWriter<'w, ReplaceDisplaySettingsRequest>,
    pub(crate) graphics_setting_requests: MessageWriter<'w, ReplaceGraphicsSettingsRequest>,
    pub(crate) zoo_cameras: Query<'w, 's, Entity, With<ZooCamera>>,
}

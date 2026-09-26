use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    settings::{
        display_settings_types::{DisplaySettings, ReplaceDisplaySettingsRequest},
        graphics_settings_types::{GraphicsSettings, ReplaceGraphicsSettingsRequest},
        online_message_policy_types::OnlineMessagePolicy,
        options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
    },
};

#[derive(SystemParam)]
pub(in crate::plugins) struct SettingsActionTargets<'w, 's> {
    pub(in crate::plugins) draft_settings:
        ResMut<'w, OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    pub(in crate::plugins) accepted_display_settings: Res<'w, DisplaySettings>,
    pub(in crate::plugins) accepted_graphics_settings: Res<'w, GraphicsSettings>,
    pub(in crate::plugins) online_message_policy: ResMut<'w, OnlineMessagePolicy>,
    pub(in crate::plugins) display_setting_requests:
        MessageWriter<'w, ReplaceDisplaySettingsRequest>,
    pub(in crate::plugins) graphics_setting_requests:
        MessageWriter<'w, ReplaceGraphicsSettingsRequest>,
    pub(in crate::plugins) zoo_cameras: Query<'w, 's, Entity, With<ZooCamera>>,
}

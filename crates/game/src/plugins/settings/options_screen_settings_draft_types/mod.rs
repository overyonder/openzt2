use bevy::prelude::*;

use super::{display_settings_types::DisplaySettings, graphics_settings_types::GraphicsSettings};

/// Options-screen working copy. Accepted renderer resources remain unchanged
/// until the authored accept action publishes replacement requests.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
    pub display: DisplaySettings,
    pub graphics: GraphicsSettings,
    pub message_of_the_day: bool,
    pub dirty_display: bool,
    pub dirty_graphics: bool,
}

impl Default for OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
    fn default() -> Self {
        Self {
            display: DisplaySettings::default(),
            graphics: GraphicsSettings::default(),
            message_of_the_day: true,
            dirty_display: false,
            dirty_graphics: false,
        }
    }
}

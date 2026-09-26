use super::graphics_settings_types::{GraphicsSettings, GraphicsSettingsRejectionReason};

pub(super) fn validate_proposed_graphics_settings(
    proposed_graphics_settings: GraphicsSettings,
) -> Result<(), GraphicsSettingsRejectionReason> {
    if !matches!(proposed_graphics_settings.multisample_count, 1 | 2 | 4 | 8) {
        return Err(GraphicsSettingsRejectionReason::UnsupportedMultisampleCount);
    }
    Ok(())
}

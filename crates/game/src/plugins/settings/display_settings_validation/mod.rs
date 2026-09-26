use super::display_settings_types::{DisplaySettings, DisplaySettingsRejectionReason};

pub(super) fn validate_proposed_display_settings(
    proposed_display_settings: DisplaySettings,
    primary_window_exists: bool,
) -> Result<(), DisplaySettingsRejectionReason> {
    if !primary_window_exists
        || proposed_display_settings.width == 0
        || proposed_display_settings.height == 0
    {
        return Err(DisplaySettingsRejectionReason::InvalidResolution);
    }
    if !(500..=2000).contains(&proposed_display_settings.ui_scale_permille) {
        return Err(DisplaySettingsRejectionReason::InvalidScale);
    }
    Ok(())
}

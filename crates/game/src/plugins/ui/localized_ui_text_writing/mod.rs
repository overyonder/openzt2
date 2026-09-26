use openzt2_game_data::{
    localization::{LocalizationFormatArgument, LocalizationFormatError},
    AssetId,
};

use crate::assets::localization::loaded_localization_queries::LoadedLocalizationView;

pub(crate) fn localized_ui_text<'a>(
    localization: LoadedLocalizationView<'a>,
    localization_key: AssetId,
) -> Option<&'a str> {
    localization.find_plain_localized_text(localization_key)
}

/// Writes loaded localization tokens directly into caller-owned storage so
/// repeated presentation updates retain the existing string allocation.
pub(crate) fn write_localized_ui_text(
    localization: LoadedLocalizationView<'_>,
    localization_key: AssetId,
    format_arguments: &[LocalizationFormatArgument<'_>],
    target_text: &mut String,
) -> Result<(), LocalizationFormatError> {
    target_text.clear();
    localization.write_localized_text_with_format_arguments(
        localization_key,
        format_arguments,
        target_text,
    )
}

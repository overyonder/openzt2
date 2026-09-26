use bevy::prelude::*;
use openzt2_game_data::{
    localization::{
        LocalizationFormatArgument, LocalizationFormatError, LocalizationRichContentNode,
        LocalizationTextPresentation,
    },
    AssetId,
};

use super::{
    fallback_localization_formatting::write_fallback_currency_amount,
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

#[derive(Clone, Copy)]
pub(crate) struct LoadedLocalizationView<'a> {
    localization_precedence_index: &'a LocalizationPrecedenceIndex,
    localization_assets: &'a Assets<LocalizationAsset>,
}

impl<'a> LoadedLocalizationView<'a> {
    pub(super) fn new(
        localization_precedence_index: &'a LocalizationPrecedenceIndex,
        localization_assets: &'a Assets<LocalizationAsset>,
    ) -> Self {
        Self {
            localization_precedence_index,
            localization_assets,
        }
    }

    pub(crate) fn find_plain_localized_text(&self, id: AssetId) -> Option<&'a str> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_plain_localized_text(id)
    }

    pub(crate) fn write_localized_text_with_format_arguments(
        &self,
        id: AssetId,
        arguments: &[LocalizationFormatArgument<'_>],
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        self.find_localization_asset_for_entry(id)
            .ok_or(LocalizationFormatError::MissingKey)?
            .localization_catalog
            .write_localized_text_with_format_arguments(id, arguments, target)
    }

    pub(crate) fn find_localized_text_presentation(
        &self,
        id: AssetId,
    ) -> Option<&'a LocalizationTextPresentation> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_localized_text_presentation(id)
    }

    pub(crate) fn find_localized_rich_content(
        &self,
        id: AssetId,
    ) -> Option<&'a [LocalizationRichContentNode]> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_localized_rich_content(id)
    }

    pub(crate) fn write_localized_currency_amount(
        &self,
        currency_amount_cents: i64,
        include_fractional_currency_cents: bool,
        target: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        if let Some(asset) =
            self.find_localization_asset_for_entry(AssetId::from_key("locale:currency_symbol"))
        {
            asset.localization_catalog.write_localized_currency_amount(
                currency_amount_cents,
                include_fractional_currency_cents,
                target,
            )
        } else {
            write_fallback_currency_amount(
                currency_amount_cents,
                include_fractional_currency_cents,
                target,
            )
        }
    }

    pub(crate) fn localized_month_abbreviation(&self, month_number: u8) -> Option<&'a str> {
        self.localization_precedence_index
            .localization_asset_handles()
            .iter()
            .rev()
            .filter_map(|handle| self.localization_assets.get(handle))
            .find_map(|asset| {
                asset
                    .localization_catalog
                    .localized_month_abbreviation(month_number)
            })
    }

    pub(crate) fn write_localized_month_and_year(
        &self,
        month: u8,
        year: u16,
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        self.localization_precedence_index
            .localization_asset_handles()
            .iter()
            .rev()
            .filter_map(|handle| self.localization_assets.get(handle))
            .find(|asset| {
                !asset
                    .localization_catalog
                    .localized_month_and_year_format_tokens
                    .is_empty()
            })
            .ok_or(LocalizationFormatError::MissingKey)?
            .localization_catalog
            .write_localized_month_and_year(month, year, target)
    }

    fn find_localization_asset_for_entry(
        &self,
        localization_entry_identifier: AssetId,
    ) -> Option<&'a LocalizationAsset> {
        self.localization_precedence_index
            .find_asset_for_localization_entry(
                self.localization_assets,
                localization_entry_identifier,
            )
    }
}

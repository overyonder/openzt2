use bevy::prelude::*;
use openzt2_game_data::{
    AssetId,
    localization::{
        LocalizationCatalog, LocalizationDateFormatToken, LocalizationFormatArgument,
        LocalizationFormatError, LocalizationRichContentNode, LocalizationTextPresentation,
    },
};

use super::{
    fallback_localization_formatting::write_fallback_currency_amount,
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

#[derive(Clone, Copy)]
pub struct LoadedLocalizationView<'a> {
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

    pub fn find_plain_localized_text(&self, id: AssetId) -> Option<&'a str> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_plain_localized_text(id)
    }

    pub fn write_localized_text_with_format_arguments(
        &self,
        id: AssetId,
        arguments: &[LocalizationFormatArgument<'_>],
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        self.find_localization_asset_for_entry(id)
            .ok_or(LocalizationFormatError::MissingKey)?
            .localization_catalog
            .write_localized_text_with_currency_formatter(id, arguments, target, |cents, writer| {
                self.write_localized_currency_amount(cents, true, writer)
            })
    }

    pub fn find_localized_text_presentation(
        &self,
        id: AssetId,
    ) -> Option<&'a LocalizationTextPresentation> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_localized_text_presentation(id)
    }

    pub fn find_localized_rich_content(
        &self,
        id: AssetId,
    ) -> Option<&'a [LocalizationRichContentNode]> {
        self.find_localization_asset_for_entry(id)?
            .localization_catalog
            .find_localized_rich_content(id)
    }

    pub fn write_localized_currency_amount(
        &self,
        currency_amount_cents: i64,
        include_fractional_currency_cents: bool,
        target: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        let symbol_id = self
            .localization_precedence_index
            .configuration()
            .currency_symbol;
        if let Some(symbol) = self.find_plain_localized_text(symbol_id) {
            LocalizationCatalog::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
                symbol,
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

    pub fn localized_month_abbreviation(&self, month_number: u8) -> Option<&'a str> {
        let month_index = usize::from(month_number.checked_sub(1)?);
        self.find_plain_localized_text(
            *self
                .localization_precedence_index
                .configuration()
                .months
                .get(month_index)?,
        )
    }

    pub fn write_localized_month_and_year(
        &self,
        month: u8,
        year: u16,
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        let tokens = self.localization_precedence_index.month_year_tokens();
        if tokens.is_empty() {
            return Err(LocalizationFormatError::MissingKey);
        }
        let month = self
            .localized_month_abbreviation(month)
            .ok_or(LocalizationFormatError::MissingKey)?;
        for token in tokens {
            match token {
                LocalizationDateFormatToken::Literal(text) => target.write_str(text),
                LocalizationDateFormatToken::MonthAbbreviation => target.write_str(month),
                LocalizationDateFormatToken::Year => write!(target, "{year:02}"),
            }
            .map_err(LocalizationFormatError::Write)?;
        }
        Ok(())
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

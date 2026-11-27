use bevy::{ecs::system::SystemParam, prelude::*};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use openzt2_game_data::{
    localization::{
        LocalizationFormatArgument, LocalizationFormatError, LocalizationRichContentNode,
        LocalizationTextPresentation,
    },
    AssetId,
};

use super::{
    blue_fang_localization_date_format_tokenization::{
        LocalizationDateTimeFormat, LocalizationDateTimeToken,
    },
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
    windows_locale_conventions::bundled_locale_conventions,
};

#[derive(SystemParam)]
pub struct CurrencyLocalization<'w> {
    index: Option<Res<'w, LocalizationPrecedenceIndex>>,
    assets: Option<Res<'w, Assets<LocalizationAsset>>>,
}

impl CurrencyLocalization<'_> {
    pub fn is_changed(&self) -> bool {
        self.index.as_ref().is_some_and(|index| index.is_changed())
            || self
                .assets
                .as_ref()
                .is_some_and(|assets| assets.is_changed())
    }

    pub fn loaded_text(&self) -> Option<LoadedLocalizationView<'_>> {
        self.index
            .as_deref()?
            .borrow_loaded_localization_view(self.assets.as_deref()?)
    }

    pub fn write_amount(
        &self,
        cents: i64,
        include_cents: bool,
        target: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        if let Some(localization) = self.loaded_text() {
            return localization.write_localized_currency_amount(cents, include_cents, target);
        }
        let currency = self.index.as_ref().map_or_else(
            || &bundled_locale_conventions(1033).currency,
            |index| &index.configuration().locale_conventions().currency,
        );
        currency.write_amount(&currency.symbol, cents, include_cents, target)
    }
}

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

    pub fn find_person_name_pool(
        &self,
        id: AssetId,
    ) -> Option<
        &'a openzt2_game_data::world_definitions::guest_simulation_definitions::PersonNamePool,
    > {
        self.localization_precedence_index
            .find_person_name_pool(self.localization_assets, id)
    }

    pub fn render_person_name(
        &self,
        pool: AssetId,
        first_row: Option<u32>,
        last_row: Option<u32>,
    ) -> Option<String> {
        let pool = self.find_person_name_pool(pool)?;
        let mut name = String::new();
        if let Some(row) = first_row {
            let first = pool.first_names.get(row as usize)?;
            if !first.is_empty() {
                name.push_str(first);
                name.push_str(&pool.delimiter);
            }
        }
        if let Some(row) = last_row {
            name.push_str(pool.last_names.get(row as usize)?);
        }
        Some(name)
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
        let configuration = self.localization_precedence_index.configuration();
        let currency = &configuration.locale_conventions().currency;
        let symbol = self
            .find_plain_localized_text(configuration.currency_symbol)
            .unwrap_or(&currency.symbol);
        currency.write_amount(
            symbol,
            currency_amount_cents,
            include_fractional_currency_cents,
            target,
        )
    }

    pub fn localized_month_abbreviation(&self, month_number: u8) -> Option<&'a str> {
        let month_index = usize::from(month_number.checked_sub(1)?);
        let configuration = self.localization_precedence_index.configuration();
        self.find_plain_localized_text(*configuration.months.get(month_index)?)
            .or_else(|| {
                configuration
                    .locale_conventions()
                    .calendar
                    .abbreviated_month_names
                    .get(month_index)
                    .map(String::as_str)
            })
    }

    pub fn write_localized_month_and_year(
        &self,
        month: u8,
        year: u16,
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        let value = NaiveDate::from_ymd_opt(i32::from(year), u32::from(month), 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .ok_or(LocalizationFormatError::MissingKey)?;
        self.write_localized_date_or_time(LocalizationDateTimeFormat::MonthYear, value, target)
    }

    pub fn write_localized_date_or_time(
        &self,
        format: LocalizationDateTimeFormat,
        value: NaiveDateTime,
        target: &mut impl std::fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        use LocalizationDateTimeToken as Token;
        let tokens = self.localization_precedence_index.date_time_tokens(format);
        if tokens.is_empty() {
            return Err(LocalizationFormatError::MissingKey);
        }
        let configuration = self.localization_precedence_index.configuration();
        let calendar = &configuration.locale_conventions().calendar;
        let month_index = value.month0() as usize;
        let weekday_index = value.weekday().num_days_from_sunday() as usize;
        let genitive_month = tokens
            .iter()
            .any(|token| matches!(token, Token::Day(1 | 2)));
        for token in tokens {
            match token {
                Token::Literal(text) => target.write_str(text),
                Token::Month(1) => write!(target, "{}", value.month()),
                Token::Month(2) => write!(target, "{:02}", value.month()),
                Token::Month(3) => target.write_str(
                    self.find_plain_localized_text(configuration.months[month_index])
                        .unwrap_or_else(|| calendar.month_name(month_index, true, genitive_month)),
                ),
                Token::Month(_) => {
                    target.write_str(calendar.month_name(month_index, false, genitive_month))
                }
                Token::Day(1) => write!(target, "{}", value.day()),
                Token::Day(2) => write!(target, "{:02}", value.day()),
                Token::Day(3) => {
                    target.write_str(&calendar.abbreviated_weekday_names[weekday_index])
                }
                Token::Day(_) => target.write_str(&calendar.weekday_names[weekday_index]),
                Token::Year(width) => {
                    let width = *width;
                    let year = if width < 3 {
                        value.year().rem_euclid(100)
                    } else {
                        value.year()
                    };
                    write!(target, "{year:0width$}")
                }
                Token::Hour12(width) => {
                    write!(target, "{:0width$}", value.hour12().1, width = *width)
                }
                Token::Hour24(width) => write!(target, "{:0width$}", value.hour(), width = *width),
                Token::Minute(width) => {
                    write!(target, "{:0width$}", value.minute(), width = *width)
                }
                Token::Second(width) => {
                    write!(target, "{:0width$}", value.second(), width = *width)
                }
                Token::Era => target.write_str(&calendar.era),
                Token::Period(width) => {
                    let designator = if value.hour() < 12 {
                        &calendar.am_designator
                    } else {
                        &calendar.pm_designator
                    };
                    if *width == 1 {
                        designator
                            .chars()
                            .next()
                            .map_or(Ok(()), |initial| target.write_char(initial))
                    } else {
                        target.write_str(designator)
                    }
                }
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

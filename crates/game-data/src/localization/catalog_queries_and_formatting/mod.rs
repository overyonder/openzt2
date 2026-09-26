use std::fmt;

use crate::AssetId;

use super::{
    LocalizationCatalog, LocalizationDateFormatToken, LocalizationEntry,
    LocalizationFormatArgument, LocalizationFormatArgumentKind, LocalizationFormatError,
    LocalizationFormatToken, LocalizationRichContentNode, LocalizationTextPresentation,
};

impl LocalizationCatalog {
    #[must_use]
    pub fn find_localization_entry(
        &self,
        localization_entry_id: AssetId,
    ) -> Option<&LocalizationEntry> {
        self.localization_entries
            .binary_search_by_key(&localization_entry_id, |entry| {
                entry.localization_entry_identifier
            })
            .ok()
            .map(|entry_index| &self.localization_entries[entry_index])
    }

    #[must_use]
    pub fn find_plain_localized_text(&self, localization_entry_id: AssetId) -> Option<&str> {
        self.find_localization_entry(localization_entry_id)
            .map(|entry| entry.plain_localized_text.as_str())
    }

    /// Writes a localized entry with typed substitutions.
    ///
    /// # Errors
    ///
    /// Returns a typed formatting error for a missing key or argument, an
    /// argument type mismatch, or a target writer failure.
    pub fn write_localized_text_with_format_arguments(
        &self,
        localization_entry_id: AssetId,
        format_arguments: &[LocalizationFormatArgument<'_>],
        target_writer: &mut impl fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        let localization_entry = self
            .find_localization_entry(localization_entry_id)
            .ok_or(LocalizationFormatError::MissingKey)?;

        for localization_token in &localization_entry.localized_text_format_tokens {
            match localization_token {
                LocalizationFormatToken::Literal(literal_text) => target_writer
                    .write_str(literal_text)
                    .map_err(LocalizationFormatError::Write)?,
                LocalizationFormatToken::Argument {
                    format_argument_index,
                    format_argument_kind,
                } => {
                    let format_argument = format_arguments
                        .get(usize::from(*format_argument_index))
                        .ok_or(LocalizationFormatError::MissingArgument(
                            *format_argument_index,
                        ))?;

                    match (format_argument_kind, format_argument) {
                        (
                            LocalizationFormatArgumentKind::Integer,
                            LocalizationFormatArgument::Integer(integer),
                        ) => {
                            write!(target_writer, "{integer}")
                        }
                        (
                            LocalizationFormatArgumentKind::Decimal,
                            LocalizationFormatArgument::Decimal(decimal),
                        ) => {
                            write!(target_writer, "{decimal}")
                        }
                        (
                            LocalizationFormatArgumentKind::Currency,
                            LocalizationFormatArgument::CurrencyCents(cents),
                        ) => self.write_localized_currency_amount(*cents, true, target_writer),
                        (
                            LocalizationFormatArgumentKind::Percent,
                            LocalizationFormatArgument::PercentBasisPoints(basis_points),
                        ) => write!(
                            target_writer,
                            "{}.{:02}%",
                            basis_points / 100,
                            basis_points.unsigned_abs() % 100
                        ),
                        (
                            LocalizationFormatArgumentKind::Text,
                            LocalizationFormatArgument::Text(text),
                        ) => target_writer.write_str(text),
                        _ => return Err(LocalizationFormatError::ArgumentType),
                    }
                    .map_err(LocalizationFormatError::Write)?;
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn find_localized_text_presentation(
        &self,
        localization_entry_id: AssetId,
    ) -> Option<&LocalizationTextPresentation> {
        self.find_localization_entry(localization_entry_id)
            .map(|entry| &entry.localized_text_presentation)
    }

    #[must_use]
    pub fn find_localized_rich_content(
        &self,
        localization_entry_id: AssetId,
    ) -> Option<&[LocalizationRichContentNode]> {
        self.find_localization_entry(localization_entry_id)
            .map(|entry| entry.localized_rich_content_nodes.as_slice())
    }

    /// Writes localized currency without allocating an intermediate string.
    ///
    /// # Errors
    ///
    /// Returns the target writer's formatting error.
    pub fn write_localized_currency_amount(
        &self,
        currency_amount_cents: i64,
        include_fractional_currency_cents: bool,
        target_writer: &mut impl fmt::Write,
    ) -> fmt::Result {
        let currency_symbol = self
            .find_plain_localized_text(AssetId::from_key("locale:currency_symbol"))
            .unwrap_or("$");

        Self::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
            currency_symbol,
            currency_amount_cents,
            include_fractional_currency_cents,
            target_writer,
        )
    }

    /// Writes currency using the native English-locale thousands grouping.
    ///
    /// # Errors
    ///
    /// Returns the target writer's formatting error.
    pub fn write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
        currency_symbol: &str,
        currency_amount_cents: i64,
        include_fractional_currency_cents: bool,
        target_writer: &mut impl fmt::Write,
    ) -> fmt::Result {
        let whole_dollars = currency_amount_cents.unsigned_abs() / 100;

        if currency_amount_cents < 0 {
            target_writer.write_str("-")?;
        }
        target_writer.write_str(currency_symbol)?;

        let mut remaining_dollars = whole_dollars;
        let mut group_divisor = 1_u64;
        while remaining_dollars >= 1_000 {
            remaining_dollars /= 1_000;
            group_divisor *= 1_000;
        }
        write!(target_writer, "{remaining_dollars}")?;
        while group_divisor > 1 {
            group_divisor /= 1_000;
            write!(
                target_writer,
                ",{:03}",
                whole_dollars / group_divisor % 1_000
            )?;
        }

        if include_fractional_currency_cents {
            write!(
                target_writer,
                ".{:02}",
                currency_amount_cents.unsigned_abs() % 100
            )?;
        }

        Ok(())
    }

    #[must_use]
    pub fn localized_month_abbreviation(&self, month_number: u8) -> Option<&str> {
        const LOCALIZATION_MONTH_KEYS: [&str; 12] = [
            "locale:january",
            "locale:february",
            "locale:march",
            "locale:april",
            "locale:may",
            "locale:june",
            "locale:july",
            "locale:august",
            "locale:september",
            "locale:october",
            "locale:november",
            "locale:december",
        ];

        month_number
            .checked_sub(1)
            .and_then(|month_index| LOCALIZATION_MONTH_KEYS.get(usize::from(month_index)))
            .and_then(|month_key| self.find_plain_localized_text(AssetId::from_key(month_key)))
    }

    /// Writes a localized month and year without allocating an intermediate string.
    ///
    /// # Errors
    ///
    /// Returns a typed error for an invalid month or target writer failure.
    pub fn write_localized_month_and_year(
        &self,
        month_number: u8,
        year: u16,
        target_writer: &mut impl fmt::Write,
    ) -> Result<(), LocalizationFormatError> {
        let month_abbreviation = self
            .localized_month_abbreviation(month_number)
            .ok_or(LocalizationFormatError::MissingKey)?;

        for month_year_token in &self.localized_month_and_year_format_tokens {
            match month_year_token {
                LocalizationDateFormatToken::Literal(literal_text) => {
                    target_writer.write_str(literal_text)
                }
                LocalizationDateFormatToken::MonthAbbreviation => {
                    target_writer.write_str(month_abbreviation)
                }
                LocalizationDateFormatToken::Year => write!(target_writer, "{year:02}"),
            }
            .map_err(LocalizationFormatError::Write)?;
        }

        Ok(())
    }
}

impl fmt::Display for LocalizationFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => formatter.write_str("missing localization key"),
            Self::MissingArgument(argument_index) => {
                write!(formatter, "missing localization argument {argument_index}")
            }
            Self::ArgumentType => formatter.write_str("localization argument type mismatch"),
            Self::Write(write_error) => write_error.fmt(formatter),
        }
    }
}

impl std::error::Error for LocalizationFormatError {}

#[cfg(test)]
mod tests {
    use super::LocalizationCatalog;

    #[test]
    fn currency_amounts_use_native_comma_thousands_grouping() -> std::fmt::Result {
        let mut output = String::new();
        LocalizationCatalog::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
            "$",
            -12_345_678,
            true,
            &mut output,
        )?;
        assert_eq!(output, "-$123,456.78");
        Ok(())
    }
}

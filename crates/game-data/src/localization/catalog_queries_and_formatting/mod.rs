use std::fmt;

use crate::AssetId;

use super::{
    LocalizationCatalog, LocalizationEntry, LocalizationFormatArgument,
    LocalizationFormatArgumentKind, LocalizationFormatError, LocalizationFormatToken,
    LocalizationRichContentNode, LocalizationTextPresentation,
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

    /// Formats an entry using the supplied currency formatter.
    ///
    /// # Errors
    ///
    /// Returns a formatting error for an absent key, invalid argument or writer failure.
    pub fn write_localized_text_with_currency_formatter<W: fmt::Write>(
        &self,
        localization_entry_id: AssetId,
        format_arguments: &[LocalizationFormatArgument<'_>],
        target_writer: &mut W,
        mut currency: impl FnMut(i64, &mut W) -> fmt::Result,
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
                        ) => currency(*cents, target_writer),
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

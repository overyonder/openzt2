use std::{collections::BTreeMap, io};

use openzt2_game_data::{
    localization::{LocalizationCatalog, LocalizationEntry},
    AssetId,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocument;

use super::{
    blue_fang_localization_date_format_tokenization::tokenize_blue_fang_localization_date_format,
    blue_fang_localization_entry_lowering::lower_blue_fang_localization_entries,
    blue_fang_localization_locale_recognition::locale_identifier_from_blue_fang_source_path,
};

#[derive(Default)]
pub(super) struct BlueFangLocalizationCatalogLowering {
    localization_entries_by_locale: BTreeMap<String, BTreeMap<AssetId, LocalizationEntry>>,
}

impl BlueFangLocalizationCatalogLowering {
    pub(super) fn add_source_document(
        &mut self,
        source_document: &OrderedSourceDocument,
    ) -> io::Result<()> {
        let locale_identifier =
            locale_identifier_from_blue_fang_source_path(source_document.path.as_str())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "localization path {} has no recognized locale",
                            source_document.path.as_str()
                        ),
                    )
                })?;
        let localization_entries_for_locale = self
            .localization_entries_by_locale
            .entry(locale_identifier)
            .or_default();
        for localization_entry in lower_blue_fang_localization_entries(source_document)? {
            localization_entries_for_locale.insert(
                localization_entry.localization_entry_identifier,
                localization_entry,
            );
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Vec<(String, LocalizationCatalog)> {
        self.localization_entries_by_locale
            .into_iter()
            .map(|(locale_identifier, localization_entries)| {
                let localization_entries = localization_entries.into_values().collect::<Vec<_>>();
                let localized_month_and_year_format_tokens = localization_entries
                    .iter()
                    .find(|localization_entry| {
                        localization_entry
                            .authored_localization_key
                            .eq_ignore_ascii_case("locale:month_year")
                    })
                    .map_or_else(Vec::new, |localization_entry| {
                        tokenize_blue_fang_localization_date_format(
                            &localization_entry.plain_localized_text,
                        )
                    });
                let localization_catalog = LocalizationCatalog {
                    locale_identifier: locale_identifier.clone(),
                    localization_entries,
                    localized_month_and_year_format_tokens,
                };
                (locale_identifier, localization_catalog)
            })
            .collect()
    }
}

use std::{
    io,
    path::{Path, PathBuf},
};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::AssetId;
use z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup;

use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub(super) const LOCALE_CONFIGURATION_PATH: &str = "config/locale.xml";

#[derive(Asset, TypePath, Clone, PartialEq, Eq)]
pub(super) struct LocalizationConfiguration {
    locale_id: String,
    string_table_root: PathBuf,
    pub(super) currency_symbol: AssetId,
    pub(super) month_year: AssetId,
    pub(super) months: [AssetId; 12],
}

impl Default for LocalizationConfiguration {
    fn default() -> Self {
        Self {
            locale_id: "1033".into(),
            string_table_root: "lang".into(),
            currency_symbol: AssetId::from_key("locale:currency_symbol"),
            month_year: AssetId::from_key("locale:month_year"),
            months: MONTH_NAMES
                .map(|month| AssetId::from_key(&format!("locale:{}", month.to_ascii_lowercase()))),
        }
    }
}

impl LocalizationConfiguration {
    pub(super) fn selects_string_table(&self, path: &Path) -> bool {
        if !path.extension().is_some_and(|extension| extension == "xml") {
            return false;
        }
        let Some(directory) = path
            .strip_prefix(&self.string_table_root)
            .ok()
            .and_then(|relative| relative.iter().next())
            .and_then(|locale| locale.to_str())
        else {
            return false;
        };
        directory.eq_ignore_ascii_case(&self.locale_id)
            || (self.locale_id == "1033" && directory.eq_ignore_ascii_case("oem1"))
    }
}

#[derive(Default, TypePath)]
pub(super) struct LocalizationConfigurationLoader;

impl AssetLoader for LocalizationConfigurationLoader {
    type Asset = LocalizationConfiguration;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document =
            parse_blue_fang_source_document(AssetPath::new(LOCALE_CONFIGURATION_PATH), &bytes)
                .map_err(io::Error::other)?;
        let root = document.root;
        if !root.name.eq_ignore_ascii_case("BFLocaleMgr") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected BFLocaleMgr",
            ));
        }
        let mut configuration = LocalizationConfiguration::default();
        if let Some(locale) = root.attribute_named_any(&["LocaleID"]) {
            configuration.locale_id = locale.trim().to_ascii_lowercase();
        }
        if let Some(directory) = root.attribute_named_any(&["StringTableRoot"]) {
            configuration.string_table_root =
                normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(
                    directory.trim(),
                ));
        }
        for node in root.element_children() {
            if node.name.eq_ignore_ascii_case("Currency") {
                if let Some(symbol) = node.attribute_named_any(&["CurrencySymbol"]) {
                    configuration.currency_symbol = AssetId::from_key(symbol);
                }
            } else if node.name.eq_ignore_ascii_case("DateFormats") {
                if let Some(format) = node.attribute_named_any(&["MonthYear"]) {
                    configuration.month_year = AssetId::from_key(format);
                }
            } else if node.name.eq_ignore_ascii_case("MonthAbbreviations") {
                for (month, name) in configuration.months.iter_mut().zip(MONTH_NAMES) {
                    if let Some(key) = node.attribute_named_any(&[name]) {
                        *month = AssetId::from_key(key);
                    }
                }
            }
        }
        Ok(configuration)
    }
    fn extensions(&self) -> &[&str] {
        &[
            "xml",
            crate::asset_source::archive_entry_asset_path::ARCHIVE_ENTRY_ASSET_EXTENSION,
        ]
    }
}

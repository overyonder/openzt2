use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use openzt2_game_data::AssetId;
use z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup;

use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

use super::windows_locale_conventions::{
    WindowsLocaleConventions, bundled_locale_conventions, host_locale_conventions,
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
    locale_number: u32,
    host_conventions: Option<Arc<WindowsLocaleConventions>>,
    string_table_root: PathBuf,
    pub(super) currency_symbol: AssetId,
    pub(super) date_time_formats: [AssetId; 4],
    pub(super) months: [AssetId; 12],
}

impl Default for LocalizationConfiguration {
    fn default() -> Self {
        Self {
            locale_id: "1033".into(),
            locale_number: 1033,
            host_conventions: host_locale_conventions(1033).map(Arc::new),
            string_table_root: "lang".into(),
            currency_symbol: AssetId::from_key("locale:currency_symbol"),
            date_time_formats: [
                "locale:month_year",
                "locale:day_date",
                "locale:month_day_year",
                "locale:minutes_seconds",
            ]
            .map(AssetId::from_key),
            months: MONTH_NAMES
                .map(|month| AssetId::from_key(&format!("locale:{}", month.to_ascii_lowercase()))),
        }
    }
}

impl LocalizationConfiguration {
    pub(super) fn locale_conventions(&self) -> &WindowsLocaleConventions {
        self.host_conventions
            .as_deref()
            .unwrap_or_else(|| bundled_locale_conventions(self.locale_number))
    }

    pub(super) fn string_table_root(&self) -> &Path {
        &self.string_table_root
    }

    pub(super) fn table_locale<'a>(&self, path: &'a Path) -> Option<&'a str> {
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        {
            return None;
        }
        let directory = path
            .strip_prefix(&self.string_table_root)
            .ok()?
            .iter()
            .next()?
            .to_str()?;
        (directory.eq_ignore_ascii_case(&self.locale_id)
            || (self.locale_number == 1033 && directory.eq_ignore_ascii_case("oem1")))
        .then_some(directory)
    }

    pub(super) fn selects_string_table(&self, path: &Path) -> bool {
        self.table_locale(path).is_some()
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
            configuration.locale_number = configuration
                .locale_id
                .parse()
                .or_else(|_| {
                    u32::from_str_radix(configuration.locale_id.trim_start_matches("0x"), 16)
                })
                .unwrap_or(1033);
            if configuration.locale_number != 1033 {
                configuration.host_conventions =
                    host_locale_conventions(configuration.locale_number).map(Arc::new);
            }
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
                for (id, field) in configuration.date_time_formats.iter_mut().zip([
                    "MonthYear",
                    "DayDate",
                    "MonthDayYear",
                    "MinutesSeconds",
                ]) {
                    if let Some(key) = node.attribute_named_any(&[field]) {
                        *id = AssetId::from_key(key);
                    }
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

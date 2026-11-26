use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    prelude::*,
};

use openzt2_game_data::localization::LocalizationCatalog;

use super::{
    blue_fang_localization_entry_lowering::lower_blue_fang_localization_entries,
    localization_asset_types::LocalizationAsset,
    localization_configuration::{LocalizationConfiguration, LocalizationConfigurationLoader},
};
use crate::{
    asset_source::archive_entry_asset_path::{
        archive_entry_asset_source_path, ARCHIVE_ENTRY_ASSET_EXTENSION,
    },
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

#[derive(Default, serde::Deserialize, serde::Serialize)]
pub(super) struct LocalizationTableSettings {
    pub(super) locale: String,
}

#[derive(Default, TypePath)]
struct LocalizationAssetLoader;

impl AssetLoader for LocalizationAssetLoader {
    type Asset = LocalizationAsset;
    type Settings = LocalizationTableSettings;
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let asset_path = load_context.path().path();
        let archive_path = archive_entry_asset_source_path(asset_path).map(|(path, _, _)| path);
        let source_path = archive_path.as_deref().unwrap_or(asset_path);
        let source_document =
            parse_blue_fang_source_document(AssetPath::new(&source_path.to_string_lossy()), &bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

        let locale_identifier = settings.locale.clone();
        let mut entries = std::collections::BTreeMap::new();
        let mut person_name_pools = std::collections::BTreeMap::new();
        lower_blue_fang_localization_entries(
            &source_document,
            &locale_identifier,
            &mut entries,
            &mut person_name_pools,
        )?;
        Ok(LocalizationAsset {
            localization_catalog: LocalizationCatalog {
                locale_identifier,
                localization_entries: entries.into_values().collect(),
                localized_month_and_year_format_tokens: Vec::new(),
            },
            person_name_pools,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml", ARCHIVE_ENTRY_ASSET_EXTENSION]
    }
}

pub(super) fn register_localization_catalog_asset_and_loader(application: &mut App) {
    application
        .init_asset::<LocalizationConfiguration>()
        .init_asset_loader::<LocalizationConfigurationLoader>()
        .init_asset::<LocalizationAsset>()
        .init_asset_loader::<LocalizationAssetLoader>();
}

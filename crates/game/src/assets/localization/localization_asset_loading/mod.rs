use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    prelude::*,
};

use super::{
    blue_fang_localization_catalog_lowering::BlueFangLocalizationCatalogLowering,
    localization_asset_types::LocalizationAsset,
};
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[derive(Default, TypePath)]
struct LocalizationAssetLoader;

impl AssetLoader for LocalizationAssetLoader {
    type Asset = LocalizationAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source_path = load_context.path().path().to_string_lossy();
        let source_document = parse_blue_fang_source_document(AssetPath::new(&source_path), &bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let mut lowering = BlueFangLocalizationCatalogLowering::default();
        lowering.add_source_document(&source_document)?;
        let localization_catalog = lowering
            .finish()
            .into_iter()
            .find(|(locale, _)| locale.eq_ignore_ascii_case("en-US"))
            .map(|(_, localization_catalog)| localization_catalog)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "source document contains no English localization",
                )
            })?;
        Ok(LocalizationAsset::from_localization_catalog(
            localization_catalog,
        ))
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

pub(super) fn register_localization_catalog_asset_and_loader(application: &mut App) {
    application
        .init_asset::<LocalizationAsset>()
        .init_asset_loader::<LocalizationAssetLoader>();
}

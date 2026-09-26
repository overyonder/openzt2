use std::io;

use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    prelude::*,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub(super) struct D3d9EffectSourceAsset;

#[derive(TypePath)]
pub(super) struct D3d9EffectSourceAssetLoader;

impl AssetLoader for D3d9EffectSourceAssetLoader {
    type Asset = D3d9EffectSourceAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut source_bytes = Vec::new();
        reader.read_to_end(&mut source_bytes).await?;
        Ok(D3d9EffectSourceAsset)
    }

    fn extensions(&self) -> &[&str] {
        &["fx", "fxh", "h"]
    }
}

pub(super) fn register_d3d9_effect_source_asset(application: &mut App) {
    application.init_asset::<D3d9EffectSourceAsset>();
}

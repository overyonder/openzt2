//! Per-document audio asset loading and precedence-aware handle indexing.

mod audio_asset_index_refresh;
pub(crate) mod audio_asset_types;
mod audio_document_asset_loader;
mod source;

use bevy::{asset::AssetApp, prelude::*};

use self::{
    audio_asset_index_refresh::{
        index_loaded_audio_assets, refresh_audio_assets_from_archive_overlay,
    },
    audio_asset_types::{AudioAsset, AudioAssets},
    audio_document_asset_loader::AudioDocumentAssetLoader,
};

pub(super) struct AudioAssetPlugin;

impl Plugin for AudioAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<AudioAsset>()
            .init_asset_loader::<AudioDocumentAssetLoader>()
            .init_resource::<AudioAssets>()
            .add_systems(
                PreUpdate,
                (
                    refresh_audio_assets_from_archive_overlay,
                    index_loaded_audio_assets,
                )
                    .chain(),
            );
    }
}

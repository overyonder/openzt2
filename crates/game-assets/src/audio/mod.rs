//! Per-document audio asset loading and precedence-aware handle indexing.

pub mod audio_asset_index_refresh;
pub mod audio_asset_types;
pub mod audio_document_asset_loader;
pub mod source;

use bevy::{asset::AssetApp, prelude::*};

use self::{
    audio_asset_index_refresh::{discover_audio_catalogue_sources, publish_loaded_audio_rows},
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
                    discover_audio_catalogue_sources
                        .run_if(resource_changed::<crate::asset_source::AssetArchives>),
                    publish_loaded_audio_rows,
                )
                    .chain()
                    .after(bevy::asset::AssetTrackingSystems),
            );
    }
}

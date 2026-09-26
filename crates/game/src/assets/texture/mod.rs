//! Image asset registration.

mod blue_fang_source_image_decoding;
mod compatible_source_image_asset_loading;
pub mod interactive_texture_metadata_asset_and_borrowing_queries;
mod interactive_texture_metadata_asset_loading;
pub(crate) mod source_image_asset_path_selection;

use bevy::prelude::*;

use compatible_source_image_asset_loading::CompatibleSourceImageAssetLoader;
use interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset;
use interactive_texture_metadata_asset_loading::InteractiveTextureMetadataAssetLoader;

/// Registers image compatibility and interaction metadata. Bevy continues to
/// own ordinary image loading and GPU preparation.
pub struct TextureAssetPlugin;

impl Plugin for TextureAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<InteractiveTextureMetadataAsset>()
            .init_asset_loader::<InteractiveTextureMetadataAssetLoader>()
            .preregister_asset_loader::<CompatibleSourceImageAssetLoader>(&["z2cur", "z2dds"]);
    }

    fn finish(&self, application: &mut App) {
        // Device compressed-format support exists only after the renderer
        // has been created, so the loader is constructed here.
        application.init_asset_loader::<CompatibleSourceImageAssetLoader>();
    }
}

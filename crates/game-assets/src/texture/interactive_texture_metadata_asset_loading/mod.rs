//! Interactive alpha-mask and cursor-atlas metadata loading.

use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::image::InteractiveImageMetadata;

use super::{
    blue_fang_source_image_decoding::decode_blue_fang_source_image_for_interaction,
    interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
};

pub(crate) const INTERACTIVE_TEXTURE_ASSET_EXTENSION: &str = "z2interactive";

#[derive(Default, TypePath)]
pub(super) struct InteractiveTextureMetadataAssetLoader;

impl AssetLoader for InteractiveTextureMetadataAssetLoader {
    type Asset = InteractiveTextureMetadataAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut source_bytes = Vec::new();
        reader.read_to_end(&mut source_bytes).await?;
        let asset_path = load_context.path().path();
        let source_path = if asset_path
            .extension()
            .is_some_and(|extension| extension == INTERACTIVE_TEXTURE_ASSET_EXTENSION)
        {
            asset_path.with_extension("")
        } else {
            asset_path.to_path_buf()
        };
        let mut decoded_source_image = decode_blue_fang_source_image_for_interaction(
            &source_path.to_string_lossy(),
            &source_bytes,
        )?;
        let cursor_atlas_image =
            decoded_source_image
                .cursor_atlas_image
                .take()
                .map(|cursor_atlas_image| {
                    load_context.add_labeled_asset("interactive-atlas", cursor_atlas_image)
                });
        if let Some(selected_cursor_image) = decoded_source_image.selected_cursor_image.take() {
            load_context.add_labeled_asset("interactive-image", selected_cursor_image);
        }
        Ok(InteractiveTextureMetadataAsset {
            metadata: InteractiveImageMetadata {
                alpha_hit_mask: Some(decoded_source_image.alpha_hit_mask),
                cursor_atlas: decoded_source_image.cursor_atlas,
            },
            cursor_atlas_image,
        })
    }

    fn extensions(&self) -> &[&str] {
        &[INTERACTIVE_TEXTURE_ASSET_EXTENSION]
    }
}

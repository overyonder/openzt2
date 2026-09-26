use std::{io, sync::Arc};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    gltf::{GltfLoader, GltfLoaderSettings},
    prelude::*,
};

use crate::assets::{
    model::bevy_gltf_loader_configuration::create_bevy_gltf_loader_for_lowered_source_assets,
    model_source::native_animation_source_lowering::lower_native_animation_source,
    source_document::animation_text_keys::parse_blue_fang_animation_text_key_sidecar_source,
};

use super::animation_set_asset_types::AnimationClipSourceAsset;

#[derive(TypePath)]
pub(super) struct AnimationClipSourceAssetLoader {
    bevy_gltf_loader: GltfLoader,
    asset_archives: crate::asset_source::AssetArchives,
}

impl FromWorld for AnimationClipSourceAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            bevy_gltf_loader: create_bevy_gltf_loader_for_lowered_source_assets(world),
        }
    }
}

impl AssetLoader for AnimationClipSourceAssetLoader {
    type Asset = AnimationClipSourceAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        animation_clip_source_reader: &mut dyn Reader,
        _animation_clip_loader_settings: &Self::Settings,
        animation_clip_load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .asset_archives
            .measure_scene_loading_asset_translation("animation_clip");
        let mut animation_source_bytes = Vec::new();
        animation_clip_source_reader
            .read_to_end(&mut animation_source_bytes)
            .await?;
        let animation_source_asset_path = animation_clip_load_context
            .path()
            .path()
            .to_string_lossy()
            .replace('\\', "/");
        let text_key_sidecar_asset_path =
            std::path::Path::new(&animation_source_asset_path).with_extension("txtkeys");
        let authored_animation_text_keys = match animation_clip_load_context
            .read_asset_bytes(text_key_sidecar_asset_path.clone())
            .await
        {
            Ok(text_key_sidecar_source_bytes) => parse_blue_fang_animation_text_key_sidecar_source(
                &text_key_sidecar_asset_path.to_string_lossy(),
                &text_key_sidecar_source_bytes,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(bevy::asset::ReadAssetBytesError::AssetReaderError(
                bevy::asset::io::AssetReaderError::NotFound(_),
            )) => Vec::new(),
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        let (gltf_binary_bytes, authored_animation_clip_metadata) = lower_native_animation_source(
            &animation_source_asset_path,
            &animation_source_bytes,
            &authored_animation_text_keys,
        )
        .map_err(io::Error::other)?;
        let loaded_animation_gltf = GltfLoader::load_gltf(
            &self.bevy_gltf_loader,
            &gltf_binary_bytes,
            animation_clip_load_context,
            &GltfLoaderSettings::default(),
        )
        .await
        .map_err(io::Error::other)?;
        let animation_clip_handle = loaded_animation_gltf
            .animations
            .into_iter()
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "animation source contains no clip",
                )
            })?;

        Ok(AnimationClipSourceAsset {
            authored_animation_clip_metadata: Arc::new(authored_animation_clip_metadata),
            animation_clip_handle,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["bf", "kf"]
    }
}

use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use super::{
    animation_set_asset_types::AnimationSetAsset,
    bfm_animation_set_lowering::lower_bfm_source_to_animation_set_document,
    kfm_animation_set_lowering::lower_kfm_source_to_animation_set_document,
};

#[derive(Default, TypePath)]
pub(super) struct AnimationSetSourceAssetLoader;

impl AssetLoader for AnimationSetSourceAssetLoader {
    type Asset = AnimationSetAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_animation_set", skip_all)
    )]
    async fn load(
        &self,
        animation_set_source_reader: &mut dyn Reader,
        _animation_set_loader_settings: &Self::Settings,
        animation_set_load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut animation_set_source_bytes = Vec::new();
        animation_set_source_reader
            .read_to_end(&mut animation_set_source_bytes)
            .await?;
        let animation_set_asset_path = animation_set_load_context
            .path()
            .path()
            .to_string_lossy()
            .replace('\\', "/");
        let animation_set_document = if std::path::Path::new(&animation_set_asset_path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kfm"))
        {
            lower_kfm_source_to_animation_set_document(
                &animation_set_asset_path,
                &animation_set_source_bytes,
            )?
        } else {
            lower_bfm_source_to_animation_set_document(
                &animation_set_asset_path,
                &animation_set_source_bytes,
            )?
        };
        Ok(AnimationSetAsset {
            model_asset_path: animation_set_document.model_asset_path,
            skeleton_asset_path: animation_set_document.skeleton_asset_path,
            animation_clips: Vec::new(),
            animation_graph_nodes: animation_set_document.animation_graph_nodes,
            animation_graph_order: animation_set_document.animation_graph_order,
            animation_graph_edges: animation_set_document.animation_graph_edges,
            animation_graph_metadata: animation_set_document.animation_graph_metadata,
            animation_clip_source_references: animation_set_document.animation_clips,
            pending_animation_clip_source_asset_handles: Vec::new(),
            failed_animation_clip_asset_keys: Vec::new(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["bfm", "kfm"]
    }
}

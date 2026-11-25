use std::{io, path::Path};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use super::{
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
    bfm_animation_set_lowering::lower_bfm_source_to_animation_set_document,
    kfm_animation_set_lowering::lower_kfm_source_to_animation_set_document,
};
use crate::{
    asset_source::AssetArchives, model_source::native_animation_source_lowering::kf_sequence_label,
};

#[derive(TypePath)]
pub(super) struct AnimationSetSourceAssetLoader(Option<AssetArchives>);

impl FromWorld for AnimationSetSourceAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self(world.get_resource::<AssetArchives>().cloned())
    }
}

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
        let animation_set_document = if Path::new(&animation_set_asset_path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kfm"))
        {
            let mut document = lower_kfm_source_to_animation_set_document(
                &animation_set_asset_path,
                &animation_set_source_bytes,
            )?;
            if let Some(archives) = &self.0 {
                let owner = Path::new(&animation_set_asset_path);
                let primary_sequence = kf_sequence_label(0);
                for clip in &mut document.animation_clips {
                    let Some((path, label)) = clip.animation_clip_asset_path.split_once('#') else {
                        continue;
                    };
                    if label != primary_sequence
                        || archives.resolve_source_reference(owner, path).is_some()
                    {
                        continue;
                    }
                    let compiled_path = Path::new(path).with_extension("bf");
                    if let Some(selected) =
                        archives.resolve_source_reference(owner, &compiled_path.to_string_lossy())
                    {
                        clip.animation_clip_asset_path =
                            selected.to_string_lossy().replace('\\', "/");
                    }
                }
            }
            document
        } else {
            lower_bfm_source_to_animation_set_document(
                &animation_set_asset_path,
                &animation_set_source_bytes,
            )?
        };
        let preloaded_clip_sources = animation_set_document
            .animation_clips
            .iter()
            .filter(|reference| {
                reference.playback_policy.load_during_animation_set_load == Some(true)
            })
            .map(|reference| {
                let source = animation_set_load_context
                    .load::<AnimationClipSourceAsset>(reference.animation_clip_asset_path.clone());
                (reference.animation_clip_asset_key.clone(), source)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut set = AnimationSetAsset::create_ready_animation_set_asset(
            animation_set_document.model_asset_path,
            animation_set_document.skeleton_asset_path,
            Vec::new(),
            animation_set_document.animation_graph_nodes,
            animation_set_document.animation_graph_order,
            animation_set_document.animation_graph_edges,
            animation_set_document.animation_graph_metadata,
        );
        set.set_animation_clip_source_references(animation_set_document.animation_clips);
        set.preloaded_clip_sources = preloaded_clip_sources;
        Ok(set)
    }

    fn extensions(&self) -> &[&str] {
        &["bfm", "kfm"]
    }
}

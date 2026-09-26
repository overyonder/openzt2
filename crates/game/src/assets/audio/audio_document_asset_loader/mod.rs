use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::audio::VARIANT_CUE_REFERENCE;

use crate::{
    asset_source::AssetArchives,
    assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

use super::{audio_asset_types::AudioAsset, source};

#[derive(TypePath)]
pub(super) struct AudioDocumentAssetLoader {
    archives: AssetArchives,
}

impl FromWorld for AudioDocumentAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for AudioDocumentAssetLoader {
    type Asset = AudioAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source_path = context.path().path();
        let path = source_path.to_string_lossy().replace('\\', "/");
        let document = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let lowered = source::lower(&document, &|from, authored| {
            self.archives
                .resolve_audio_reference(from, authored)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} {:?}: {}", error.path, error.span, error.message),
            )
        })?;
        let mut clip_paths = lowered
            .cues
            .iter()
            .flat_map(|cue| &cue.variants)
            .filter(|variant| variant.flags & VARIANT_CUE_REFERENCE == 0)
            .map(|variant| (variant.source_id, variant.source.clone().into_boxed_str()))
            .collect::<Vec<_>>();
        clip_paths.sort_unstable_by_key(|(id, _)| *id);
        clip_paths.dedup_by_key(|(id, _)| *id);
        Ok(AudioAsset {
            document: lowered,
            clip_paths: clip_paths.into_boxed_slice(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

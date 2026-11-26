use bevy::{asset::LoadContext, image::ImageLoaderSettings, prelude::*};
use std::path::Path;

pub(crate) enum GeneratedAssetPublication<'a, 'load> {
    Labeled(&'a mut LoadContext<'load>),
    Runtime {
        server: &'a AssetServer,
        source: &'a Path,
    },
}

impl GeneratedAssetPublication<'_, '_> {
    pub(crate) fn source_path(&self) -> &Path {
        match self {
            Self::Labeled(context) => context.path().path(),
            Self::Runtime { source, .. } => source,
        }
    }

    pub(crate) fn publish<A: Asset>(&mut self, label: String, asset: A) -> Handle<A> {
        match self {
            Self::Labeled(context) => context.add_labeled_asset(label, asset),
            Self::Runtime { server, .. } => server.add(asset),
        }
    }

    pub(crate) fn load<A: Asset>(
        &mut self,
        path: impl Into<bevy::asset::AssetPath<'static>>,
    ) -> Handle<A> {
        match self {
            Self::Labeled(context) => context.load(path),
            Self::Runtime { server, .. } => server.load(path),
        }
    }

    pub(crate) fn load_image(&mut self, path: String, is_srgb: bool) -> Handle<Image> {
        let settings = move |settings: &mut ImageLoaderSettings| settings.is_srgb = is_srgb;
        match self {
            Self::Labeled(context) => context.load_builder().with_settings(settings).load(path),
            Self::Runtime { server, .. } => {
                server.load_builder().with_settings(settings).load(path)
            }
        }
    }

    pub(crate) async fn read_source(
        &mut self,
        archives: &super::AssetArchives,
        path: &Path,
    ) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Labeled(context) => context
                .read_asset_bytes(path.to_path_buf())
                .await
                .map_err(|error| std::io::Error::other(format!("{}: {error}", path.display()))),
            Self::Runtime { .. } => archives.read_source(path),
        }
    }
}

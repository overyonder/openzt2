use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

pub(crate) const GROUND_PATH_SURFACE_IMAGE_EXTENSION: &str = "ground-path-image";

/// A source texture and its decoded image, reloaded together by the asset server.
#[derive(Asset, TypePath)]
pub struct GroundPathSurfaceImage {
    #[dependency]
    pub image: Handle<Image>,
}

#[derive(Default, TypePath)]
pub(super) struct GroundPathSurfaceImageLoader;

impl AssetLoader for GroundPathSurfaceImageLoader {
    type Asset = GroundPathSurfaceImage;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        (): &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let source = context.path().path().with_extension("");
        let image = context
            .load_builder()
            .load_value_from_reader::<Image>(source, reader)
            .await
            .map_err(std::io::Error::other)?;
        Ok(GroundPathSurfaceImage {
            image: context.add_loaded_labeled_asset("image", image),
        })
    }

    fn extensions(&self) -> &[&str] {
        &[GROUND_PATH_SURFACE_IMAGE_EXTENSION]
    }
}

#[cfg(test)]
mod tests;

//! Bevy image loading with the narrow Blue Fang DDS and cursor compatibility path.

use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    image::{
        CompressedImageFormatSupport, CompressedImageFormats, ImageArrayLayout,
        ImageLoaderSettings, ImageType,
    },
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

use super::blue_fang_source_image_decoding::{
    decode_blue_fang_cursor_source, decode_blue_fang_dds_source_to_rgba_surface,
};

#[derive(TypePath)]
pub(super) struct CompatibleSourceImageAssetLoader {
    supported_compressed_formats: CompressedImageFormats,
    asset_archives: crate::asset_source::AssetArchives,
}

impl FromWorld for CompatibleSourceImageAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            supported_compressed_formats: world
                .get_resource::<CompressedImageFormatSupport>()
                .map_or(CompressedImageFormats::NONE, |formats| formats.0),
        }
    }
}

impl AssetLoader for CompatibleSourceImageAssetLoader {
    type Asset = Image;
    type Settings = ImageLoaderSettings;
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .asset_archives
            .measure_scene_loading_asset_translation("compatible_image");
        let mut source_bytes = Vec::new();
        reader.read_to_end(&mut source_bytes).await?;
        let extension = load_context
            .path()
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut image = if extension == "z2cur" {
            decode_blue_fang_cursor_source(&load_context.path().to_string(), &source_bytes)?
                .cursor_atlas_image
                .ok_or_else(|| io::Error::other("cursor contains no atlas image"))?
        } else {
            Image::from_buffer(
                &source_bytes,
                ImageType::Extension(&extension),
                self.supported_compressed_formats,
                settings.is_srgb,
                settings.sampler.clone(),
                settings.asset_usage,
            )
            .or_else(|error| {
                if !matches!(extension.as_str(), "dds" | "z2dds") {
                    return Err(io::Error::other(error));
                }
                // Formats the device cannot sample directly, and Blue Fang
                // variants Bevy does not parse, are decoded on the CPU.
                let decoded_surface = decode_blue_fang_dds_source_to_rgba_surface(&source_bytes)?;
                let mut image = Image::new_uninit(
                    Extent3d {
                        width: decoded_surface.width,
                        height: decoded_surface.height,
                        depth_or_array_layers: decoded_surface.array_layer_count,
                    },
                    TextureDimension::D2,
                    if settings.is_srgb {
                        TextureFormat::Rgba8UnormSrgb
                    } else {
                        TextureFormat::Rgba8Unorm
                    },
                    settings.asset_usage,
                );
                image.texture_descriptor.mip_level_count = decoded_surface.mip_level_count;
                image.data = Some(decoded_surface.rgba_data);
                if decoded_surface.is_complete_cubemap {
                    image.texture_view_descriptor = Some(TextureViewDescriptor {
                        dimension: Some(TextureViewDimension::Cube),
                        ..default()
                    });
                }
                Ok(image)
            })?
        };
        image.sampler = settings.sampler.clone();
        if let Some(texture_format) = settings.texture_format {
            image.texture_descriptor.format = texture_format;
        }
        image = match settings.array_layout {
            Some(ImageArrayLayout::RowCount { rows }) => {
                image
                    .reinterpret_stacked_2d_as_array(rows)
                    .map_err(io::Error::other)?;
                image
            }
            Some(ImageArrayLayout::RowHeight { pixels }) => {
                image
                    .reinterpret_stacked_2d_as_array(image.height() / pixels)
                    .map_err(io::Error::other)?;
                image
            }
            Some(ImageArrayLayout::GridCount { columns, rows }) => image
                .create_stacked_array_from_2d_grid(rows, columns)
                .map_err(io::Error::other)?,
            Some(ImageArrayLayout::GridSize {
                tile_width_pixels,
                tile_height_pixels,
            }) => image
                .create_stacked_array_from_2d_grid(
                    image.height() / tile_height_pixels,
                    image.width() / tile_width_pixels,
                )
                .map_err(io::Error::other)?,
            None => image,
        };
        Ok(image)
    }

    fn extensions(&self) -> &[&str] {
        &["z2cur", "z2dds"]
    }
}

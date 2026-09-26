//! CPU decoding required for Blue Fang DDS compatibility and interactive images.

use std::{io, io::Cursor, path::Path};

use bevy::{asset::RenderAssetUsages, prelude::Image};
use openzt2_game_data::image::{ImageAlphaHitMask, ImageCursorAtlas, ImageCursorAtlasEntry};

pub(super) struct DecodedInteractiveSourceImage {
    pub(super) alpha_hit_mask: ImageAlphaHitMask,
    pub(super) cursor_atlas: Option<ImageCursorAtlas>,
    pub(super) selected_cursor_image: Option<Image>,
    pub(super) cursor_atlas_image: Option<Image>,
}

pub(super) fn decode_blue_fang_source_image_for_interaction(
    source_path: &str,
    source_bytes: &[u8],
) -> io::Result<DecodedInteractiveSourceImage> {
    let extension = Path::new(source_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "cur" {
        return decode_blue_fang_cursor_source(source_path, source_bytes);
    }
    let rgba_image = decode_blue_fang_source_image_to_rgba(source_path, source_bytes, &extension)?;
    Ok(DecodedInteractiveSourceImage {
        alpha_hit_mask: create_alpha_hit_mask_from_rgba_image(&rgba_image),
        cursor_atlas: None,
        selected_cursor_image: None,
        cursor_atlas_image: None,
    })
}

fn decode_blue_fang_source_image_to_rgba(
    source_path: &str,
    source_bytes: &[u8],
    extension: &str,
) -> io::Result<image::RgbaImage> {
    match extension {
        "dds" => decode_blue_fang_dds_source_to_rgba(source_bytes),
        "bmp" => image::load_from_memory_with_format(source_bytes, image::ImageFormat::Bmp)
            .map_err(io::Error::other)
            .map(image::DynamicImage::into_rgba8),
        "jpg" | "jpeg" => {
            image::load_from_memory_with_format(source_bytes, image::ImageFormat::Jpeg)
                .map_err(io::Error::other)
                .map(image::DynamicImage::into_rgba8)
        }
        "png" => image::load_from_memory_with_format(source_bytes, image::ImageFormat::Png)
            .map_err(io::Error::other)
            .map(image::DynamicImage::into_rgba8),
        "tga" => image::load_from_memory_with_format(source_bytes, image::ImageFormat::Tga)
            .map_err(io::Error::other)
            .map(image::DynamicImage::into_rgba8),
        _ => Err(io::Error::other(format!("unsupported image {source_path}"))),
    }
}

/// Every array layer and mip level of one DDS source, decoded to RGBA8 in
/// layer-major order.
pub(super) struct DecodedBlueFangDdsRgbaSurface {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) array_layer_count: u32,
    pub(super) mip_level_count: u32,
    pub(super) is_complete_cubemap: bool,
    pub(super) rgba_data: Vec<u8>,
}

fn read_blue_fang_dds_source(source_bytes: &[u8]) -> io::Result<image_dds::ddsfile::Dds> {
    let mut dds =
        image_dds::ddsfile::Dds::read(&mut Cursor::new(source_bytes)).map_err(io::Error::other)?;
    // ddsfile discards the red-channel mask for luminance surfaces while its
    // own D3D-format matcher requires it. Restore that source-header fact.
    if dds
        .header
        .spf
        .flags
        .contains(image_dds::ddsfile::PixelFormatFlags::LUMINANCE)
        && dds.header.spf.r_bit_mask.is_none()
    {
        dds.header.spf.r_bit_mask = Some(0xff);
    }
    Ok(dds)
}

/// CPU decoding for DDS sources whose format the render device cannot
/// sample directly. Mip levels and cubemap faces are preserved.
pub(super) fn decode_blue_fang_dds_source_to_rgba_surface(
    source_bytes: &[u8],
) -> io::Result<DecodedBlueFangDdsRgbaSurface> {
    let dds = read_blue_fang_dds_source(source_bytes)?;
    let cubemap_flags =
        image_dds::ddsfile::Caps2::CUBEMAP | image_dds::ddsfile::Caps2::CUBEMAP_ALLFACES;
    let is_complete_cubemap = dds.header.caps2.contains(cubemap_flags);
    if dds.get_d3d_format() != Some(image_dds::ddsfile::D3DFormat::L8) && dds.get_depth() <= 1 {
        if let Ok(surface) = image_dds::SurfaceRgba8::decode_dds(&dds) {
            return Ok(DecodedBlueFangDdsRgbaSurface {
                width: surface.width,
                height: surface.height,
                array_layer_count: surface.layers,
                mip_level_count: surface.mipmaps,
                is_complete_cubemap: is_complete_cubemap && surface.layers == 6,
                rgba_data: surface.data,
            });
        }
    }
    let rgba_image = decode_blue_fang_dds_to_rgba(&dds)?;
    Ok(DecodedBlueFangDdsRgbaSurface {
        width: rgba_image.width(),
        height: rgba_image.height(),
        array_layer_count: 1,
        mip_level_count: 1,
        is_complete_cubemap: false,
        rgba_data: rgba_image.into_raw(),
    })
}

pub(super) fn decode_blue_fang_dds_source_to_rgba(
    source_bytes: &[u8],
) -> io::Result<image::RgbaImage> {
    decode_blue_fang_dds_to_rgba(&read_blue_fang_dds_source(source_bytes)?)
}

fn decode_blue_fang_dds_to_rgba(dds: &image_dds::ddsfile::Dds) -> io::Result<image::RgbaImage> {
    if dds.get_d3d_format() == Some(image_dds::ddsfile::D3DFormat::L8) {
        let width = usize::try_from(dds.get_width())
            .map_err(|_| io::Error::other("DDS L8 width exceeds usize"))?;
        let height = usize::try_from(dds.get_height())
            .map_err(|_| io::Error::other("DDS L8 height exceeds usize"))?;
        let pitch = dds
            .header
            .pitch
            .map(usize::try_from)
            .transpose()
            .map_err(|_| io::Error::other("DDS L8 pitch exceeds usize"))?
            .unwrap_or(width);
        if pitch < width {
            return Err(io::Error::other("DDS L8 pitch is narrower than its width"));
        }
        let required_source_bytes = pitch
            .checked_mul(height)
            .ok_or_else(|| io::Error::other("DDS L8 dimensions overflow"))?;
        let source_surface = dds
            .data
            .get(..required_source_bytes)
            .ok_or_else(|| io::Error::other("DDS L8 surface is truncated"))?;
        let rgba_pixels = source_surface
            .chunks_exact(pitch)
            .flat_map(|row| {
                row[..width]
                    .iter()
                    .flat_map(|value| [*value, *value, *value, 255])
            })
            .collect();
        return image::RgbaImage::from_raw(dds.get_width(), dds.get_height(), rgba_pixels)
            .ok_or_else(|| io::Error::other("invalid DDS L8 surface"));
    }
    image_dds::image_from_dds(&dds, 0).map_err(io::Error::other)
}

pub(super) fn decode_blue_fang_cursor_source(
    source_path: &str,
    source_bytes: &[u8],
) -> io::Result<DecodedInteractiveSourceImage> {
    let cursor_directory =
        ico::IconDir::read(Cursor::new(source_bytes)).map_err(io::Error::other)?;
    let selected_cursor_index = cursor_directory
        .entries()
        .iter()
        .enumerate()
        .max_by_key(|(_, entry)| u64::from(entry.width()) * u64::from(entry.height()))
        .map(|(index, _)| index)
        .ok_or_else(|| io::Error::other(format!("{source_path}: cursor contains no images")))?;
    let decoded_cursor_images = cursor_directory
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let decoded_icon = entry.decode().map_err(io::Error::other)?;
            let rgba_image = image::RgbaImage::from_raw(
                decoded_icon.width(),
                decoded_icon.height(),
                decoded_icon.rgba_data().to_vec(),
            )
            .ok_or_else(|| {
                io::Error::other(format!("{source_path}: invalid cursor image {index}"))
            })?;
            let hotspot = entry.cursor_hotspot().ok_or_else(|| {
                io::Error::other(format!(
                    "{source_path}: cursor image {index} has no hotspot"
                ))
            })?;
            Ok((rgba_image, hotspot))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let cursor_atlas_width =
        decoded_cursor_images
            .iter()
            .try_fold(0_u32, |width, (cursor_image, _)| {
                width.checked_add(cursor_image.width()).ok_or_else(|| {
                    io::Error::other(format!("{source_path}: cursor atlas width overflow"))
                })
            })?;
    let cursor_atlas_height = decoded_cursor_images
        .iter()
        .map(|(cursor_image, _)| cursor_image.height())
        .max()
        .ok_or_else(|| io::Error::other(format!("{source_path}: cursor contains no images")))?;
    let mut cursor_atlas_rgba_image =
        image::RgbaImage::new(cursor_atlas_width, cursor_atlas_height);
    let mut cursor_atlas_origin_x = 0_u32;
    let cursor_atlas_entries = decoded_cursor_images
        .iter()
        .map(|(cursor_image, hotspot)| {
            image::imageops::overlay(
                &mut cursor_atlas_rgba_image,
                cursor_image,
                i64::from(cursor_atlas_origin_x),
                0,
            );
            let cursor_atlas_entry = ImageCursorAtlasEntry {
                origin: [cursor_atlas_origin_x, 0],
                size: [cursor_image.width(), cursor_image.height()],
                hotspot: [hotspot.0, hotspot.1],
            };
            cursor_atlas_origin_x += cursor_image.width();
            cursor_atlas_entry
        })
        .collect();
    let selected_cursor_rgba_image = &decoded_cursor_images[selected_cursor_index].0;
    Ok(DecodedInteractiveSourceImage {
        alpha_hit_mask: create_alpha_hit_mask_from_rgba_image(selected_cursor_rgba_image),
        cursor_atlas: Some(ImageCursorAtlas {
            atlas_path: source_path.to_owned(),
            entries: cursor_atlas_entries,
        }),
        selected_cursor_image: Some(Image::from_dynamic(
            image::DynamicImage::ImageRgba8(selected_cursor_rgba_image.clone()),
            true,
            RenderAssetUsages::default(),
        )),
        cursor_atlas_image: Some(Image::from_dynamic(
            image::DynamicImage::ImageRgba8(cursor_atlas_rgba_image),
            true,
            RenderAssetUsages::default(),
        )),
    })
}

fn create_alpha_hit_mask_from_rgba_image(rgba_image: &image::RgbaImage) -> ImageAlphaHitMask {
    let pixel_count = rgba_image.as_raw().len() / 4;
    let mut alpha_hit_mask_bits = vec![0; pixel_count.div_ceil(8)];
    rgba_image.pixels().enumerate().for_each(|(index, pixel)| {
        if pixel.0[3] > 0x40 {
            alpha_hit_mask_bits[index / 8] |= 1 << (index % 8);
        }
    });
    ImageAlphaHitMask {
        width: rgba_image.width(),
        height: rgba_image.height(),
        bits: alpha_hit_mask_bits,
    }
}

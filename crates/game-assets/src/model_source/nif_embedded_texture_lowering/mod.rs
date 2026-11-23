//! Embedded NetImmerse pixel data as labelled Bevy textures.

use std::io;

use bevy::{
    asset::RenderAssetUsages,
    prelude::Image,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::netimmerse_nif_source::{
    block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
};

const EMBEDDED_NIF_TEXTURE_LABEL_PREFIX: &str = "Texture/NiPixelData/";

pub(super) fn embedded_nif_texture_label(reference: i32) -> String {
    format!("{EMBEDDED_NIF_TEXTURE_LABEL_PREFIX}{reference}")
}

pub(super) fn referenced_embedded_nif_texture(
    source_path: &str,
    texture_path: &str,
) -> Option<i32> {
    texture_path
        .strip_prefix(source_path)?
        .strip_prefix('#')?
        .strip_prefix(EMBEDDED_NIF_TEXTURE_LABEL_PREFIX)?
        .parse()
        .ok()
}

pub(super) fn lower_embedded_nif_texture(
    document: &NetImmerseNifDocument,
    reference: i32,
) -> io::Result<Image> {
    let invalid = |detail| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: NiPixelData {reference}: {detail}",
                document.source_path
            ),
        )
    };
    let Some(NetImmerseNifBlockPayload::NiPixelData(pixels)) =
        document.block(reference).map(|block| &block.payload)
    else {
        return Err(invalid("missing pixel data"));
    };
    let base = pixels
        .mipmaps
        .first()
        .ok_or_else(|| invalid("no mip levels"))?;
    let maximum_mips = u32::BITS - base.width.max(base.height).leading_zeros();
    if base.width == 0 || base.height == 0 || pixels.mipmaps.len() > maximum_mips as usize {
        return Err(invalid("invalid base dimensions or mip count"));
    }
    let format = &pixels.pixel_format;
    let compressed_format = match format.pixel_format {
        4 => Some((image_dds::ImageFormat::BC1RgbaUnorm, 8)),
        5 => Some((image_dds::ImageFormat::BC2RgbaUnorm, 16)),
        6 => Some((image_dds::ImageFormat::BC3RgbaUnorm, 16)),
        _ => None,
    };
    let palette = match format.pixel_format {
        2 | 3 => match document
            .block(pixels.palette_ref)
            .map(|block| &block.payload)
        {
            Some(NetImmerseNifBlockPayload::NiPalette(colors)) => Some(colors),
            _ => return Err(invalid("missing pixel palette")),
        },
        _ => None,
    };
    let bytes_per_pixel = pixels.bytes_per_pixel as usize;
    let masks = [
        format.red_mask,
        format.green_mask,
        format.blue_mask,
        format.alpha_mask,
    ];
    if compressed_format.is_none()
        && palette.is_none()
        && (!matches!(format.pixel_format, 0 | 1)
            || !(1..=4).contains(&bytes_per_pixel)
            || masks[..3].contains(&0))
    {
        return Err(invalid(
            "unsupported pixel format or missing RGB channel masks",
        ));
    }
    let mut rgba = Vec::new();
    for (level, mip) in pixels.mipmaps.iter().enumerate() {
        if mip.width != (base.width >> level).max(1) || mip.height != (base.height >> level).max(1)
        {
            return Err(invalid("noncanonical mip dimensions"));
        }
        let (width, height, stride) = if let Some((_, stride)) = compressed_format {
            (mip.width.div_ceil(4), mip.height.div_ceil(4), stride)
        } else {
            (
                mip.width,
                mip.height,
                if palette.is_some() {
                    1
                } else {
                    bytes_per_pixel
                },
            )
        };
        let start = mip.offset as usize;
        let end = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(stride))
            .and_then(|size| start.checked_add(size))
            .ok_or_else(|| invalid("mip level size overflow"))?;
        let data = pixels
            .pixel_data
            .get(start..end)
            .ok_or_else(|| invalid("truncated mip level"))?;
        if let Some((image_format, _)) = compressed_format {
            let decoded = image_dds::Surface {
                width: mip.width,
                height: mip.height,
                depth: 1,
                layers: 1,
                mipmaps: 1,
                image_format,
                data,
            }
            .decode_rgba8()
            .map_err(io::Error::other)?;
            rgba.extend(decoded.data);
        } else if let Some(palette) = palette {
            for &index in data {
                let mut color = *palette
                    .get(usize::from(index))
                    .ok_or_else(|| invalid("invalid palette index"))?;
                if format.pixel_format == 2 {
                    color[3] = 255;
                }
                rgba.extend(color);
            }
        } else {
            for pixel in data.chunks_exact(bytes_per_pixel) {
                let mut packed = [0; 4];
                packed[..bytes_per_pixel].copy_from_slice(pixel);
                let packed = u32::from_le_bytes(packed);
                rgba.extend(masks.map(|mask| unpack_color_channel(packed, mask)));
            }
        }
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: base.width,
            height: base.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.mip_level_count = pixels.mipmaps.len() as u32;
    image.data = Some(rgba);
    Ok(image)
}

fn unpack_color_channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let maximum = u64::from(mask >> shift);
    let channel = u64::from((pixel & mask) >> shift);
    ((channel * 255 + maximum / 2) / maximum) as u8
}

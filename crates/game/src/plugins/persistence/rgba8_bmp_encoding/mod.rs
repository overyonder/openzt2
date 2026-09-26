use std::io;

pub(super) fn encode_tightly_packed_rgba8_as_bgra_bmp(
    image_width_pixels: u32,
    image_height_pixels: u32,
    rgba8_pixel_bytes: &[u8],
) -> io::Result<Vec<u8>> {
    let pixel_byte_count = image_width_pixels
        .checked_mul(image_height_pixels)
        .and_then(|pixel_count| pixel_count.checked_mul(4))
        .and_then(|byte_count| usize::try_from(byte_count).ok())
        .ok_or_else(|| io::Error::other("image dimensions overflow"))?;
    if rgba8_pixel_bytes.len() != pixel_byte_count {
        return Err(io::Error::other("image is not tightly packed RGBA8"));
    }
    if image_width_pixels == 0
        || image_height_pixels == 0
        || image_width_pixels > i32::MAX as u32
        || image_height_pixels > i32::MAX as u32
    {
        return Err(io::Error::other("image dimensions exceed BMP limits"));
    }
    let mut bitmap_bytes = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut bitmap_bytes)
        .encode(
            rgba8_pixel_bytes,
            image_width_pixels,
            image_height_pixels,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(io::Error::other)?;
    Ok(bitmap_bytes)
}

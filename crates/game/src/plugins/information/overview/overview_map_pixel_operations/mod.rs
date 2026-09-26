use bevy::prelude::*;

use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

const OVERVIEW_MAP_IMAGE_EXTENT_PIXELS: u32 = 512;

pub(super) fn draw_overview_map_line_pixels(
    overview_map_pixels: &mut [u8],
    mut current_pixel: IVec2,
    end_pixel: IVec2,
    line_pixel_color: [u8; 4],
) {
    let line_axis_distances = (end_pixel - current_pixel).abs();
    let line_axis_steps = IVec2::new(
        if current_pixel.x < end_pixel.x { 1 } else { -1 },
        if current_pixel.y < end_pixel.y { 1 } else { -1 },
    );
    let mut line_error = line_axis_distances.x - line_axis_distances.y;
    loop {
        set_overview_map_pixel_color(overview_map_pixels, current_pixel, line_pixel_color);
        if current_pixel == end_pixel {
            break;
        }
        let doubled_line_error = line_error * 2;
        if doubled_line_error > -line_axis_distances.y {
            line_error -= line_axis_distances.y;
            current_pixel.x += line_axis_steps.x;
        }
        if doubled_line_error < line_axis_distances.x {
            line_error += line_axis_distances.x;
            current_pixel.y += line_axis_steps.y;
        }
    }
}

pub(super) fn rasterize_world_sample_area_into_overview_map_pixels(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    world_position: Vec2,
    world_sample_spacing: f32,
    sample_pixel_color: [u8; 4],
) {
    let center_pixel = convert_world_position_to_overview_map_pixel(world_bounds, world_position);
    let world_bounds_size = world_bounds.max - world_bounds.min;
    let sample_pixel_radius = ((world_sample_spacing / world_bounds_size.min_element()
        * OVERVIEW_MAP_IMAGE_EXTENT_PIXELS as f32
        * 0.5)
        .ceil() as i32)
        .max(1);
    for pixel_y_offset in -sample_pixel_radius..=sample_pixel_radius {
        for pixel_x_offset in -sample_pixel_radius..=sample_pixel_radius {
            set_overview_map_pixel_color(
                overview_map_pixels,
                center_pixel + IVec2::new(pixel_x_offset, pixel_y_offset),
                sample_pixel_color,
            );
        }
    }
}

pub(super) fn convert_world_position_to_overview_map_pixel(
    world_bounds: &WorldTerrainHorizontalBounds,
    world_position: Vec2,
) -> IVec2 {
    let world_bounds_size = world_bounds.max - world_bounds.min;
    let normalized_world_position = (world_position - world_bounds.min) / world_bounds_size;
    (normalized_world_position * (OVERVIEW_MAP_IMAGE_EXTENT_PIXELS - 1) as f32)
        .clamp(
            Vec2::ZERO,
            Vec2::splat((OVERVIEW_MAP_IMAGE_EXTENT_PIXELS - 1) as f32),
        )
        .round()
        .as_ivec2()
}

fn set_overview_map_pixel_color(
    overview_map_pixels: &mut [u8],
    pixel_position: IVec2,
    pixel_color: [u8; 4],
) {
    if pixel_position.x < 0
        || pixel_position.y < 0
        || pixel_position.x >= OVERVIEW_MAP_IMAGE_EXTENT_PIXELS as i32
        || pixel_position.y >= OVERVIEW_MAP_IMAGE_EXTENT_PIXELS as i32
    {
        return;
    }
    let pixel_byte_index = (pixel_position.y as usize * OVERVIEW_MAP_IMAGE_EXTENT_PIXELS as usize
        + pixel_position.x as usize)
        * 4;
    if let Some(destination_pixel) =
        overview_map_pixels.get_mut(pixel_byte_index..pixel_byte_index + 4)
    {
        destination_pixel.copy_from_slice(&pixel_color);
    }
}

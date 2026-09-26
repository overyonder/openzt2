use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};

#[derive(Clone, Copy)]
struct TerrainSurfacePaint {
    coordinate: IVec2,
    biome: usize,
    cover: bool,
    linked: bool,
    transition: bool,
}

pub(super) fn compose_terrain_surface_image(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited_chunks: &[(Handle<TerrainAsset>, IVec2, EditedTerrainSamples)],
    images: &Assets<Image>,
) -> Option<Image> {
    let chunk_cells = i32::from(chunk.side.checked_sub(1)?);
    let margin = i32::from(
        asset
            .canonical_terrain_grid()
            .presentation
            .surface_margin_cells,
    );
    let extent = u32::from(
        asset
            .canonical_terrain_grid()
            .presentation
            .surface_resolution,
    );
    if margin < 0 || extent == 0 {
        return None;
    }
    let origin = chunk.coord * chunk_cells - IVec2::splat(margin);
    let composed_cells = chunk_cells + margin * 2;
    let mut paints = (0..=composed_cells)
        .flat_map(|z| (0..=composed_cells).map(move |x| origin + IVec2::new(x, z)))
        .map(|coordinate| read_terrain_surface_paint(chunk, asset, edited_chunks, coordinate))
        .collect::<Option<Vec<_>>>()?;
    let side = usize::try_from(composed_cells + 1).ok()?;
    let snapshot = paints.clone();
    for (index, paint) in paints.iter_mut().enumerate() {
        let x = index % side;
        let z = index / side;
        paint.transition = (z.saturating_sub(1)..=(z + 1).min(side - 1)).any(|nz| {
            (x.saturating_sub(1)..=(x + 1).min(side - 1)).any(|nx| {
                let other = snapshot[nz * side + nx];
                other.biome != paint.biome
                    || other.cover != paint.cover
                    || other.linked != paint.linked
            })
        });
    }
    paints.sort_by_key(|paint| {
        (
            paint.linked,
            paint.transition,
            asset.canonical_terrain_grid().biomes[paint.biome].draw_order,
            paint.biome,
            paint.cover,
            paint.coordinate.y,
            paint.coordinate.x,
        )
    });
    let mut rgba = vec![0; extent as usize * extent as usize * 4];
    for paint in paints {
        paint_terrain_surface_stamp(
            asset,
            images,
            origin,
            composed_cells,
            extent,
            paint,
            &mut rgba,
        )?;
    }
    Some(Image::new(
        Extent3d {
            width: extent,
            height: extent,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ))
}

fn read_terrain_surface_paint(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited_chunks: &[(Handle<TerrainAsset>, IVec2, EditedTerrainSamples)],
    coordinate: IVec2,
) -> Option<TerrainSurfacePaint> {
    let source_coordinate = IVec2::new(coordinate.x, -coordinate.y);
    let x = source_coordinate
        .x
        .clamp(0, asset.canonical_terrain_grid().width as i32 - 1) as usize;
    let z = source_coordinate
        .y
        .clamp(0, asset.canonical_terrain_grid().height as i32 - 1) as usize;
    let global = z
        .checked_mul(asset.canonical_terrain_grid().width as usize)?
        .checked_add(x)?;
    let chunk_cells = usize::from(chunk.side.checked_sub(1)?);
    let source_owner = IVec2::new(
        (x / chunk_cells).min(asset.canonical_terrain_grid().sector_columns as usize - 1) as i32,
        (z / chunk_cells).min(asset.canonical_terrain_grid().sector_rows as usize - 1) as i32,
    );
    let owner = IVec2::new(source_owner.x, -source_owner.y - 1);
    let local_x = x - source_owner.x as usize * chunk_cells;
    let source_local_z = z - source_owner.y as usize * chunk_cells;
    let local_z = chunk_cells.checked_sub(source_local_z)?;
    let local = local_z * chunk.side as usize + local_x;
    let edited = edited_chunks
        .iter()
        .find(|(handle, coord, _)| handle == &chunk.asset && *coord == owner)
        .map(|(_, _, edited)| edited);
    let base = asset.canonical_terrain_grid().samples.get(global)?;
    let biome = edited
        .and_then(|edited| edited.blend_weights.get(local * 16..local * 16 + 16))
        .and_then(|weights| {
            weights
                .iter()
                .enumerate()
                .max_by_key(|(index, weight)| (**weight, std::cmp::Reverse(*index)))
                .map(|(index, _)| index)
        })
        .unwrap_or(usize::from(base.biome_index));
    let cover = edited
        .and_then(|edited| edited.ground_cover.get(local).copied())
        .unwrap_or(u8::from(base.has_ground_cover))
        != 0;
    Some(TerrainSurfacePaint {
        coordinate,
        biome,
        cover,
        linked: base.height_is_linked,
        transition: false,
    })
}

fn paint_terrain_surface_stamp(
    asset: &TerrainAsset,
    images: &Assets<Image>,
    origin: IVec2,
    cells: i32,
    extent: u32,
    paint: TerrainSurfacePaint,
    output: &mut [u8],
) -> Option<()> {
    let biome = asset.canonical_terrain_grid().biomes.get(paint.biome)?;
    let selector = (paint.coordinate.x * 13 + paint.coordinate.y * 7 + 20).rem_euclid(4) as usize;
    let brush_path = match (paint.transition, paint.cover) {
        (false, false) => biome.ground_no_blend_brushes.get(selector),
        (false, true) => biome.cover_no_blend_brushes.get(selector),
        (true, false) => biome.ground_brushes.get(selector),
        (true, true) => biome.cover_brushes.get(selector),
    }?;
    let texture_path = if paint.cover {
        &biome.cover_texture
    } else {
        &biome.ground_texture
    };
    let brush = images.get(asset.texture_image(brush_path)?)?;
    let texture = images.get(asset.texture_image(texture_path)?)?;
    let brush_extent = read_rgba_image_extent(brush)?;
    let texture_extent = read_rgba_image_extent(texture)?;
    let divisor = if asset
        .canonical_terrain_grid()
        .presentation
        .full_alpha_composition
    {
        1
    } else {
        2
    };
    let stamp = UVec2::new(
        brush_extent.x.div_ceil(divisor),
        brush_extent.y.div_ceil(divisor),
    );
    let center = IVec2::new(
        (paint.coordinate.x - origin.x) * extent as i32 / cells,
        (paint.coordinate.y - origin.y) * extent as i32 / cells,
    );
    let corner = center - IVec2::new(stamp.x as i32 / 2, stamp.y as i32 / 2);
    for y in 0..stamp.y {
        for x in 0..stamp.x {
            let destination = corner + IVec2::new(x as i32, y as i32);
            if destination.cmplt(IVec2::ZERO).any()
                || destination.cmpge(IVec2::splat(extent as i32)).any()
            {
                continue;
            }
            let mask = read_wrapped_rgba_image_pixel(
                brush,
                x * brush_extent.x / stamp.x,
                y * brush_extent.y / stamp.y,
            )?;
            let coverage =
                ((u16::from(mask[0]) * 54 + u16::from(mask[1]) * 183 + u16::from(mask[2]) * 19)
                    >> 8) as u8;
            if asset
                .canonical_terrain_grid()
                .presentation
                .full_alpha_composition
            {
                if coverage <= 10 {
                    continue;
                }
            } else if coverage <= 99 {
                continue;
            }
            let texels_per_cell = if asset
                .canonical_terrain_grid()
                .presentation
                .full_alpha_composition
            {
                8
            } else {
                4
            };
            let source_x = (paint.coordinate.x.rem_euclid(texture_extent.x as i32) as u32
                * texels_per_cell
                + x)
                % texture_extent.x;
            let source_y = (paint.coordinate.y.rem_euclid(texture_extent.y as i32) as u32
                * texels_per_cell
                + y)
                % texture_extent.y;
            let source = read_wrapped_rgba_image_pixel(texture, source_x, source_y)?;
            let target = (destination.y as usize * extent as usize + destination.x as usize) * 4;
            if !asset
                .canonical_terrain_grid()
                .presentation
                .full_alpha_composition
                || coverage >= 246
            {
                output[target..target + 4].copy_from_slice(&source);
            } else {
                let alpha = u16::from(coverage);
                output[target..target + 4].iter_mut().zip(source).for_each(
                    |(destination, source)| {
                        *destination = ((u16::from(*destination) * (255 - alpha)
                            + u16::from(source) * alpha)
                            / 255) as u8;
                    },
                );
            }
        }
    }
    Some(())
}

/// Shipped path masks use L8 DDS, which Bevy retains as one byte per texel.
fn read_ground_path_mask_coverage(image: &Image, x: u32, y: u32) -> Option<u8> {
    if matches!(
        image.texture_descriptor.format,
        TextureFormat::R8Uint | TextureFormat::R8Unorm
    ) {
        let extent = read_rgba_image_extent(image)?;
        let offset = ((y % extent.y) * extent.x + x % extent.x) as usize;
        return image.data.as_ref()?.get(offset).copied();
    }
    let mask = read_wrapped_rgba_image_pixel(image, x, y)?;
    Some(
        ((u16::from(mask[0]) * 54 + u16::from(mask[1]) * 183 + u16::from(mask[2]) * 19) >> 8) as u8,
    )
}

#[cfg(test)]
mod ground_path_mask_tests {
    use super::*;

    #[test]
    fn single_channel_path_mask_reads_coverage_without_rgba_stride() {
        let image = Image::new(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0, 64, 128, 255],
            TextureFormat::R8Uint,
            RenderAssetUsages::MAIN_WORLD,
        );
        assert_eq!(read_ground_path_mask_coverage(&image, 0, 0), Some(0));
        assert_eq!(read_ground_path_mask_coverage(&image, 1, 0), Some(64));
        assert_eq!(read_ground_path_mask_coverage(&image, 0, 1), Some(128));
        assert_eq!(read_ground_path_mask_coverage(&image, 3, 3), Some(255));
    }
}

fn read_rgba_image_extent(image: &Image) -> Option<UVec2> {
    let size = image.texture_descriptor.size;
    (size.width != 0 && size.height != 0).then_some(UVec2::new(size.width, size.height))
}

pub(super) fn read_wrapped_rgba_image_pixel(image: &Image, x: u32, y: u32) -> Option<[u8; 4]> {
    let extent = read_rgba_image_extent(image)?;
    let offset = ((y % extent.y) * extent.x + x % extent.x) as usize * 4;
    let bytes: [u8; 4] = image
        .data
        .as_ref()?
        .get(offset..offset + 4)?
        .try_into()
        .ok()?;
    match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Some(bytes),
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            Some([bytes[2], bytes[1], bytes[0], bytes[3]])
        }
        _ => None,
    }
}

/// Blends a ground-path patch into its terrain chunk image. Source and mask coordinates use
/// the native 256-texel-per-24-cell world lattice; the destination image also
/// includes the terrain chunk's authored margin cells.
pub(super) fn blend_ground_path_surface_patch_into_terrain_surface_image(
    surface: &mut Image,
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    path_cell_world_origin: Vec2,
    path_texture: &Image,
    detail_mask: &Image,
    mirror_x: bool,
    mirror_y: bool,
) -> Option<()> {
    const NATIVE_PATH_SURFACE_PATCH_TEXELS: i32 = 43;

    let chunk_cells = f32::from(chunk.side.checked_sub(1)?);
    let margin_cells = f32::from(
        asset
            .canonical_terrain_grid()
            .presentation
            .surface_margin_cells,
    );
    let extent = read_rgba_image_extent(surface)?;
    let output_origin = chunk.origin - Vec2::splat(margin_cells * chunk.spacing_m);
    let output_texel_step = Vec2::new(
        (chunk_cells + margin_cells * 2.0) * chunk.spacing_m / extent.x as f32,
        (chunk_cells + margin_cells * 2.0) * chunk.spacing_m / extent.y as f32,
    );
    let native_texel_step = chunk_cells * chunk.spacing_m
        / f32::from(
            asset
                .canonical_terrain_grid()
                .presentation
                .surface_resolution,
        );
    if !native_texel_step.is_finite()
        || native_texel_step <= 0.0
        || output_texel_step.cmple(Vec2::ZERO).any()
    {
        return None;
    }
    let patch_world_extent = native_texel_step * NATIVE_PATH_SURFACE_PATCH_TEXELS as f32;
    // Source +Y maps to Bevy -Z. The source grid corner is therefore the
    // maximum-Z edge of this patch, not its minimum-Z edge.
    let destination_min =
        ((path_cell_world_origin - Vec2::new(0.0, patch_world_extent) - output_origin)
            / output_texel_step)
            .floor()
            .as_ivec2()
            .max(IVec2::ZERO);
    let destination_max = ((path_cell_world_origin + Vec2::new(patch_world_extent, 0.0)
        - output_origin)
        / output_texel_step)
        .ceil()
        .as_ivec2()
        .min(extent.as_ivec2());
    if destination_min.cmpge(destination_max).any() {
        return Some(());
    }
    let source_phase = (Vec2::new(path_cell_world_origin.x, -path_cell_world_origin.y)
        / native_texel_step)
        .floor()
        .as_ivec2();
    let path_texture_extent = read_rgba_image_extent(path_texture)?;
    let detail_mask_extent = read_rgba_image_extent(detail_mask)?;
    let surface_bytes = surface.data.as_mut()?;
    for destination_y in destination_min.y..destination_max.y {
        for destination_x in destination_min.x..destination_max.x {
            let destination = IVec2::new(destination_x, destination_y);
            let world = output_origin + destination.as_vec2() * output_texel_step;
            let local = (Vec2::new(
                world.x - path_cell_world_origin.x,
                path_cell_world_origin.y - world.y,
            ) / native_texel_step)
                .floor()
                .as_ivec2();
            if local.cmplt(IVec2::ZERO).any()
                || local
                    .cmpge(IVec2::splat(NATIVE_PATH_SURFACE_PATCH_TEXELS))
                    .any()
            {
                continue;
            }
            let source_coordinate = source_phase + local;
            let mask_coordinate = IVec2::new(
                if mirror_x {
                    NATIVE_PATH_SURFACE_PATCH_TEXELS - 1 - local.x
                } else {
                    local.x
                },
                if mirror_y {
                    NATIVE_PATH_SURFACE_PATCH_TEXELS - 1 - local.y
                } else {
                    local.y
                },
            );
            let coverage = read_ground_path_mask_coverage(
                detail_mask,
                mask_coordinate.x.rem_euclid(detail_mask_extent.x as i32) as u32,
                mask_coordinate.y.rem_euclid(detail_mask_extent.y as i32) as u32,
            )?;
            // The native path record selects threshold copying, not alpha
            // blending: only mask bytes greater than 99 replace the ground.
            if coverage <= 99 {
                continue;
            }
            let source = read_wrapped_rgba_image_pixel(
                path_texture,
                source_coordinate.x.rem_euclid(path_texture_extent.x as i32) as u32,
                source_coordinate.y.rem_euclid(path_texture_extent.y as i32) as u32,
            )?;
            let target = (destination_y as usize * extent.x as usize + destination_x as usize) * 4;
            surface_bytes[target..target + 4].copy_from_slice(&source);
        }
    }
    Some(())
}

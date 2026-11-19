use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::{TerrainBiome, TerrainWaterDepth};

use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainPaintedTexture;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TerrainSurfaceWater {
    Dry,
    Shore,
    Submerged,
}

#[derive(Clone, Copy)]
struct TerrainSurfacePaint {
    coordinate: IVec2,
    biome: usize,
    cover: bool,
    linked: bool,
    transition: bool,
    height_m: f32,
    water_level_m: Option<f32>,
    water: TerrainSurfaceWater,
}

// The shore band is any dry sample next to water and no higher than
// that water plus its biome shoreOffset; the native rule is unknown.
fn classify_terrain_surface_water<'a>(
    paint: &TerrainSurfacePaint,
    mut neighbours: impl Iterator<Item = &'a TerrainSurfacePaint>,
    biomes: &[TerrainBiome],
) -> TerrainSurfaceWater {
    if paint.water_level_m.is_some() {
        return TerrainSurfaceWater::Submerged;
    }
    let on_shore = neighbours.any(|other| {
        other.water_level_m.is_some_and(|level| {
            let offset_mm = biomes
                .get(other.biome)
                .and_then(|biome| biome.water_shore_offset_mm)
                .unwrap_or(0);
            paint.height_m <= level + offset_mm as f32 * 0.001
        })
    });
    if on_shore {
        TerrainSurfaceWater::Shore
    } else {
        TerrainSurfaceWater::Dry
    }
}

fn surrounding_terrain_surface_paints(
    paints: &[TerrainSurfacePaint],
    side: usize,
    index: usize,
) -> impl Iterator<Item = &TerrainSurfacePaint> {
    let x = index % side;
    let z = index / side;
    (z.saturating_sub(1)..=(z + 1).min(side - 1)).flat_map(move |nz| {
        (x.saturating_sub(1)..=(x + 1).min(side - 1)).map(move |nx| &paints[nz * side + nx])
    })
}

/// Where a chunk's composed cells sit in its surface image.
struct TerrainSurfaceLayout {
    origin: IVec2,
    composed_cells: i32,
    margin: i32,
    extent: u32,
}

pub(super) fn compose_terrain_surface_image(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited_chunks: &[(Handle<TerrainAsset>, IVec2, EditedTerrainSamples)],
    images: &Assets<Image>,
) -> Option<Image> {
    let _span = info_span!("compose_whole_terrain_surface").entered();
    let (paints, layout) = compose_terrain_surface_paints(chunk, asset, edited_chunks)?;
    let mut rgba = vec![0; layout.extent as usize * layout.extent as usize * 4];
    let whole = (IVec2::ZERO, IVec2::splat(layout.extent as i32));
    composite_terrain_surface_stamps(asset, images, &layout, &paints, whole, &mut rgba)?;
    Some(terrain_surface_image(layout.extent, rgba))
}

/// Recomposes only the part of `previous` that the changed local samples
/// (`changed_min..=changed_max`) can reach, giving the same pixels as a full
/// composition. A cell's water and transition flags read its neighbours, so
/// cells two away from a change are restamped too; every stamp overlapping
/// that region is composited again, clipped to it.
pub(super) fn recompose_terrain_surface_image_region(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited_chunks: &[(Handle<TerrainAsset>, IVec2, EditedTerrainSamples)],
    images: &Assets<Image>,
    previous: &Image,
    changed_samples: Option<(UVec2, UVec2)>,
    changed_pixels: Option<IRect>,
) -> Option<(Image, IRect)> {
    let _span = info_span!("recompose_terrain_surface_region").entered();
    let (paints, layout) = compose_terrain_surface_paints(chunk, asset, edited_chunks)?;
    let previous_data = previous.data.as_ref().filter(|data| {
        previous.texture_descriptor.format == TextureFormat::Rgba8Unorm
            && previous.texture_descriptor.size.width == layout.extent
            && previous.texture_descriptor.size.height == layout.extent
            && data.len() == layout.extent as usize * layout.extent as usize * 4
    });
    let Some(previous_data) = previous_data else {
        let image = compose_terrain_surface_image(chunk, asset, edited_chunks, images)?;
        let extent = read_rgba_image_extent(&image)?.as_ivec2();
        return Some((image, IRect::from_corners(IVec2::ZERO, extent)));
    };
    let mut region = changed_pixels.map(|bounds| (bounds.min, bounds.max));
    if let Some((changed_min, changed_max)) = changed_samples {
        let reach_min = changed_min.as_ivec2() + IVec2::splat(layout.margin - 2);
        let reach_max = changed_max.as_ivec2() + IVec2::splat(layout.margin + 2);
        for paint in &paints {
            let cell = paint.coordinate - layout.origin;
            if cell.cmplt(reach_min).any() || cell.cmpgt(reach_max).any() {
                continue;
            }
            let Some((corner, size)) = terrain_surface_stamp_bounds(asset, images, &layout, paint)
            else {
                continue;
            };
            let end = corner + size.as_ivec2();
            region =
                Some(region.map_or((corner, end), |(min, max)| (min.min(corner), max.max(end))));
        }
    }
    let Some((min, max)) = region else {
        return Some((
            previous.clone(),
            IRect::from_corners(IVec2::ZERO, IVec2::ZERO),
        ));
    };
    let clip = (
        min.max(IVec2::ZERO),
        max.min(IVec2::splat(layout.extent as i32)),
    );
    if clip.0.cmpge(clip.1).any() {
        return Some((
            previous.clone(),
            IRect::from_corners(IVec2::ZERO, IVec2::ZERO),
        ));
    }
    let mut rgba = previous_data.clone();
    // Most stamps lie far from the region, and their centres alone rule them
    // out without looking up their brushes.
    let reach = terrain_surface_stamp_reach(asset, images);
    let mut overlapping = Vec::new();
    for paint in paints {
        let center = terrain_surface_stamp_center(&layout, &paint);
        if reach.is_some_and(|reach| {
            (center + reach).cmple(clip.0).any() || (center - reach).cmpge(clip.1).any()
        }) {
            continue;
        }
        let Some((corner, size)) = terrain_surface_stamp_bounds(asset, images, &layout, &paint)
        else {
            continue;
        };
        let end = corner + size.as_ivec2();
        if end.cmple(clip.0).any() || corner.cmpge(clip.1).any() {
            continue;
        }
        overlapping.push(paint);
    }
    composite_terrain_surface_stamps(asset, images, &layout, &overlapping, clip, &mut rgba)?;
    Some((
        terrain_surface_image(layout.extent, rgba),
        IRect::from_corners(clip.0, clip.1),
    ))
}

fn terrain_surface_image(extent: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width: extent,
            height: extent,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

/// Every composed cell's paint, classified and in draw order.
fn compose_terrain_surface_paints(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited_chunks: &[(Handle<TerrainAsset>, IVec2, EditedTerrainSamples)],
) -> Option<(Vec<TerrainSurfacePaint>, TerrainSurfaceLayout)> {
    let _span = info_span!("compose_terrain_surface_paints").entered();
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
    let biomes = &asset.canonical_terrain_grid().biomes;
    let snapshot = paints.clone();
    for (index, paint) in paints.iter_mut().enumerate() {
        paint.water = classify_terrain_surface_water(
            paint,
            surrounding_terrain_surface_paints(&snapshot, side, index),
            biomes,
        );
    }
    let snapshot = paints.clone();
    for (index, paint) in paints.iter_mut().enumerate() {
        paint.transition =
            surrounding_terrain_surface_paints(&snapshot, side, index).any(|other| {
                other.biome != paint.biome
                    || other.cover != paint.cover
                    || other.linked != paint.linked
                    || other.water != paint.water
            });
    }
    paints.sort_by_key(|paint| {
        (
            paint.linked,
            paint.transition,
            asset.canonical_terrain_grid().biomes[paint.biome].draw_order,
            paint.biome,
            paint.cover,
            paint.water,
            paint.coordinate.y,
            paint.coordinate.x,
        )
    });
    Some((
        paints,
        TerrainSurfaceLayout {
            origin,
            composed_cells,
            margin,
            extent,
        },
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
    let grid = asset.canonical_terrain_grid();
    let height_m = edited
        .and_then(|edited| edited.heights_cm.get(local))
        .map_or(
            base.height_relative_to_base_metres + grid.units.base_height_metres,
            |height| f32::from(*height) * 0.01,
        );
    let water_level_m = match edited {
        Some(edited) => (*edited.water_styles.get(local)? != 0)
            .then_some(f32::from(*edited.water_cm.get(local)?) * 0.01),
        None => (base.water_depth != TerrainWaterDepth::Dry).then(|| {
            asset
                .sample_authored_water_region_index(global)
                .flatten()
                .and_then(|region| grid.water_regions.get(region))
                .map_or(height_m, |region| {
                    grid.units.base_height_metres + region.height_relative_to_base_metres
                })
        }),
    };
    Some(TerrainSurfacePaint {
        coordinate,
        biome,
        cover,
        linked: base.height_is_linked,
        transition: false,
        height_m,
        water_level_m,
        water: TerrainSurfaceWater::Dry,
    })
}

/// The brush mask and texture a paint stamps with.
fn terrain_surface_stamp_images<'a>(
    asset: &TerrainAsset,
    images: &'a Assets<Image>,
    paint: &TerrainSurfacePaint,
) -> Option<(&'a Image, &'a Image)> {
    let biome = asset.canonical_terrain_grid().biomes.get(paint.biome)?;
    let selector = (paint.coordinate.x * 13 + paint.coordinate.y * 7 + 20).rem_euclid(4) as usize;
    let water_texture = biome
        .water_presentation
        .as_ref()
        .and_then(|water| match paint.water {
            TerrainSurfaceWater::Dry => None,
            // A shore texture that didn't load, such as the unshipped benthic
            // one, falls back to the ground texture.
            TerrainSurfaceWater::Shore => water
                .shoreline
                .shore_texture
                .as_ref()
                .filter(|path| asset.texture_image(path).is_some()),
            TerrainSurfaceWater::Submerged => water.shoreline.submerged_texture.as_ref(),
        });
    let cover = paint.cover && water_texture.is_none();
    let brush_path = match (paint.transition, cover) {
        (false, false) => biome.ground_no_blend_brushes.get(selector),
        (false, true) => biome.cover_no_blend_brushes.get(selector),
        (true, false) => biome.ground_brushes.get(selector),
        (true, true) => biome.cover_brushes.get(selector),
    }?;
    let texture_path = water_texture.unwrap_or(if cover {
        &biome.cover_texture
    } else {
        &biome.ground_texture
    });
    Some((
        images.get(asset.texture_image(brush_path)?)?,
        images.get(asset.texture_image(texture_path)?)?,
    ))
}

/// The pixel rectangle (corner and size) a paint's stamp covers.
fn terrain_surface_stamp_bounds(
    asset: &TerrainAsset,
    images: &Assets<Image>,
    layout: &TerrainSurfaceLayout,
    paint: &TerrainSurfacePaint,
) -> Option<(IVec2, UVec2)> {
    let (brush, _) = terrain_surface_stamp_images(asset, images, paint)?;
    let brush_extent = read_rgba_image_extent(brush)?;
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
    let center = terrain_surface_stamp_center(layout, paint);
    Some((
        center - IVec2::new(stamp.x as i32 / 2, stamp.y as i32 / 2),
        stamp,
    ))
}

fn terrain_surface_stamp_center(
    layout: &TerrainSurfaceLayout,
    paint: &TerrainSurfacePaint,
) -> IVec2 {
    (paint.coordinate - layout.origin) * layout.extent as i32 / layout.composed_cells
}

/// The farthest any stamp reaches from its centre: half the largest brush
/// stamp, rounded up. `None` while a brush is unloaded.
fn terrain_surface_stamp_reach(asset: &TerrainAsset, images: &Assets<Image>) -> Option<IVec2> {
    let divisor = if asset
        .canonical_terrain_grid()
        .presentation
        .full_alpha_composition
    {
        1
    } else {
        2
    };
    let mut reach = UVec2::ZERO;
    for biome in &asset.canonical_terrain_grid().biomes {
        for path in biome
            .ground_brushes
            .iter()
            .chain(&biome.cover_brushes)
            .chain(&biome.ground_no_blend_brushes)
            .chain(&biome.cover_no_blend_brushes)
        {
            let extent = read_rgba_image_extent(images.get(asset.texture_image(path)?)?)?;
            reach = reach.max(UVec2::new(
                extent.x.div_ceil(divisor),
                extent.y.div_ceil(divisor),
            ));
        }
    }
    Some((reach / 2 + UVec2::ONE).as_ivec2())
}

/// Composites `paints`, given in draw order, into `output` inside `clip`
/// (min inclusive, max exclusive), over a cleared (transparent black) region.
///
/// Each pixel lies under many overlapping stamps, so each pixel takes the
/// stamps over it front to back and stops once it is opaque, keeping the
/// transparency the stamps above left. The stamps over each cell-sized block
/// of pixels are listed once, so a pixel only visits stamps that cover its
/// block, and most pixels turn opaque after a few.
fn composite_terrain_surface_stamps(
    asset: &TerrainAsset,
    images: &Assets<Image>,
    layout: &TerrainSurfaceLayout,
    paints: &[TerrainSurfacePaint],
    clip: (IVec2, IVec2),
    output: &mut [u8],
) -> Option<()> {
    const BLOCK: i32 = 8;
    struct Resolved<'a> {
        brush: *const Image,
        corner: IVec2,
        size: UVec2,
        brush_extent: UVec2,
        texture: &'a [u8],
        texture_extent: UVec2,
        texture_bgra: bool,
        texture_base: UVec2,
    }
    struct Stamp<'a> {
        corner: IVec2,
        size: IVec2,
        coverage: &'a [u8],
        /// Brush texel column, and row times width, for each stamp pixel.
        brush_columns: &'a [u32],
        brush_rows: &'a [u32],
        texture: &'a [u8],
        texture_extent: UVec2,
        texture_bgra: bool,
        /// The texture texel under the stamp's corner.
        texture_base: UVec2,
    }
    let size = (clip.1 - clip.0).max(IVec2::ZERO);
    let full_alpha = asset
        .canonical_terrain_grid()
        .presentation
        .full_alpha_composition;
    let (threshold, texels_per_cell) = if full_alpha { (10, 8) } else { (99, 4) };
    // Brush masks are read as luminance coverage, converted once per brush,
    // and stamps of one size scale a brush extent alike.
    let mut masks = bevy::platform::collections::HashMap::<*const Image, Box<[u8]>>::new();
    let mut scales =
        bevy::platform::collections::HashMap::<(UVec2, UVec2), (Box<[u32]>, Box<[u32]>)>::new();
    let mut resolved = Vec::with_capacity(paints.len());
    for paint in paints.iter().rev() {
        let (brush, texture) = terrain_surface_stamp_images(asset, images, paint)?;
        let brush_extent = read_rgba_image_extent(brush)?;
        let texture_extent = read_rgba_image_extent(texture)?;
        let (corner, stamp) = terrain_surface_stamp_bounds(asset, images, layout, paint)?;
        let (texture, texture_bgra) = rgba_image_bytes(texture, texture_extent)?;
        if !masks.contains_key(&(brush as *const Image)) {
            let (brush_data, brush_bgra) = rgba_image_bytes(brush, brush_extent)?;
            let (red, blue) = if brush_bgra { (2, 0) } else { (0, 2) };
            let texels = brush_extent.x as usize * brush_extent.y as usize;
            let mask = brush_data[..texels * 4]
                .chunks_exact(4)
                .map(|texel| {
                    ((u16::from(texel[red]) * 54
                        + u16::from(texel[1]) * 183
                        + u16::from(texel[blue]) * 19)
                        >> 8) as u8
                })
                .collect();
            masks.insert(brush as *const Image, mask);
        }
        scales.entry((brush_extent, stamp)).or_insert_with(|| {
            (
                (0..stamp.x)
                    .map(|x| x * brush_extent.x / stamp.x % brush_extent.x)
                    .collect(),
                (0..stamp.y)
                    .map(|y| y * brush_extent.y / stamp.y % brush_extent.y * brush_extent.x)
                    .collect(),
            )
        });
        resolved.push(Resolved {
            brush,
            corner,
            size: stamp,
            brush_extent,
            texture,
            texture_extent,
            texture_bgra,
            texture_base: UVec2::new(
                paint.coordinate.x.rem_euclid(texture_extent.x as i32) as u32 * texels_per_cell,
                paint.coordinate.y.rem_euclid(texture_extent.y as i32) as u32 * texels_per_cell,
            ) % texture_extent,
        });
    }
    let stamps: Vec<_> = resolved
        .iter()
        .map(|stamp| {
            let (brush_columns, brush_rows) = &scales[&(stamp.brush_extent, stamp.size)];
            Stamp {
                corner: stamp.corner,
                size: stamp.size.as_ivec2(),
                coverage: &masks[&stamp.brush],
                brush_columns,
                brush_rows,
                texture: stamp.texture,
                texture_extent: stamp.texture_extent,
                texture_bgra: stamp.texture_bgra,
                texture_base: stamp.texture_base,
            }
        })
        .collect();
    let blocks = (size + IVec2::splat(BLOCK - 1)) / BLOCK;
    let mut block_stamps = vec![Vec::<u32>::new(); (blocks.x * blocks.y).max(0) as usize];
    for (index, stamp) in stamps.iter().enumerate() {
        let first = (stamp.corner - clip.0).max(IVec2::ZERO);
        let last = (stamp.corner + stamp.size - clip.0).min(size);
        if last.cmple(first).any() {
            continue;
        }
        let first_block = first / BLOCK;
        let last_block = (last - IVec2::ONE) / BLOCK;
        for by in first_block.y..=last_block.y {
            for bx in first_block.x..=last_block.x {
                block_stamps[(by * blocks.x + bx) as usize].push(index as u32);
            }
        }
    }
    // Stamps are smaller than their textures, so a texel offset wraps at most
    // once; the remainder covers any that are not.
    let wrap = |base: u32, offset: u32, extent: u32| {
        let texel = base + offset;
        if texel < extent {
            texel
        } else if texel < extent * 2 {
            texel - extent
        } else {
            texel % extent
        }
    };
    // Bands of block rows write disjoint image rows, so they composite in
    // parallel.
    let extent = layout.extent as usize;
    let row_bytes = extent * 4;
    let mut bands = Vec::with_capacity(blocks.y.max(0) as usize);
    let mut rest = output.get_mut(clip.0.y as usize * row_bytes..)?;
    for by in 0..blocks.y {
        let rows = BLOCK.min(size.y - by * BLOCK) as usize;
        let (band, remainder) = std::mem::take(&mut rest).split_at_mut(rows * row_bytes);
        rest = remainder;
        bands.push((by, band));
    }
    let (stamps, block_stamps) = (&stamps, &block_stamps);
    bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default).scope(|scope| {
        for (by, band) in bands {
            scope.spawn(async move {
                let row = clip.0.y + by * BLOCK;
                for bx in 0..blocks.x {
                    let list = &block_stamps[(by * blocks.x + bx) as usize];
                    let block = IVec2::new(bx, by) * BLOCK;
                    for y in block.y..(block.y + BLOCK).min(size.y) {
                        for x in block.x..(block.x + BLOCK).min(size.x) {
                            let pixel = clip.0 + IVec2::new(x, y);
                            let mut left = 1.0_f32;
                            let mut colour = [0.0_f32; 4];
                            for &index in list {
                                let stamp = &stamps[index as usize];
                                let local = pixel - stamp.corner;
                                if local.cmplt(IVec2::ZERO).any() || local.cmpge(stamp.size).any() {
                                    continue;
                                }
                                let local = local.as_uvec2();
                                let coverage = stamp.coverage[(stamp.brush_rows[local.y as usize]
                                    + stamp.brush_columns[local.x as usize])
                                    as usize];
                                if coverage <= threshold {
                                    continue;
                                }
                                let texel =
                                    (wrap(stamp.texture_base.y, local.y, stamp.texture_extent.y)
                                        * stamp.texture_extent.x
                                        + wrap(
                                            stamp.texture_base.x,
                                            local.x,
                                            stamp.texture_extent.x,
                                        )) as usize
                                        * 4;
                                let texel = &stamp.texture[texel..texel + 4];
                                let source = if stamp.texture_bgra {
                                    [texel[2], texel[1], texel[0], texel[3]]
                                } else {
                                    [texel[0], texel[1], texel[2], texel[3]]
                                };
                                let alpha = if !full_alpha || coverage >= 246 {
                                    1.0
                                } else {
                                    f32::from(coverage) / 255.0
                                };
                                let weight = left * alpha;
                                for (sum, source) in colour.iter_mut().zip(source) {
                                    *sum += f32::from(source) * weight;
                                }
                                // Below half a level of 8-bit colour, nothing beneath shows.
                                left -= weight;
                                if left * 255.0 < 0.5 {
                                    break;
                                }
                            }
                            let target = ((pixel.y - row) as usize * extent + pixel.x as usize) * 4;
                            for (destination, sum) in
                                band[target..target + 4].iter_mut().zip(colour)
                            {
                                *destination = sum.round().clamp(0.0, 255.0) as u8;
                            }
                        }
                    }
                }
            });
        }
    });
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
mod shore_classification_tests {
    use super::*;

    #[test]
    fn banks_within_the_shore_offset_take_the_shore_texture() {
        let paint = |height_m, water_level_m| TerrainSurfacePaint {
            coordinate: IVec2::ZERO,
            biome: 0,
            cover: false,
            linked: false,
            transition: false,
            height_m,
            water_level_m,
            water: TerrainSurfaceWater::Dry,
        };
        let biome = TerrainBiome {
            name: String::new(),
            ground_texture: String::new(),
            cover_texture: String::new(),
            mix_mask: String::new(),
            cliff_textures: Default::default(),
            ground_brushes: Default::default(),
            cover_brushes: Default::default(),
            ground_no_blend_brushes: Default::default(),
            cover_no_blend_brushes: Default::default(),
            draw_order: 0,
            shallow_water_depth_mm: None,
            deep_water_depth_mm: None,
            water_shore_offset_mm: Some(400),
            water_presentation: None,
        };
        let biomes = [biome];
        let water = [paint(5.0, Some(6.6))];
        let classify = |paint: TerrainSurfacePaint| {
            classify_terrain_surface_water(&paint, water.iter(), &biomes)
        };
        assert!(classify(paint(5.0, Some(6.6))) == TerrainSurfaceWater::Submerged);
        assert!(classify(paint(6.9, None)) == TerrainSurfaceWater::Shore);
        assert!(classify(paint(7.1, None)) == TerrainSurfaceWater::Dry);
        assert!(
            classify_terrain_surface_water(&paint(6.0, None), std::iter::empty(), &biomes)
                == TerrainSurfaceWater::Dry
        );
    }
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

/// An RGBA or BGRA image's bytes, whether it is BGRA, and that it holds the
/// whole extent; other formats aren't composed.
fn rgba_image_bytes(image: &Image, extent: UVec2) -> Option<(&[u8], bool)> {
    let bgra = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
        _ => return None,
    };
    let data = image.data.as_deref()?;
    (data.len() >= extent.x as usize * extent.y as usize * 4).then_some((data, bgra))
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
    clip: IRect,
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
            .max(clip.min);
    let destination_max = ((path_cell_world_origin + Vec2::new(patch_world_extent, 0.0)
        - output_origin)
        / output_texel_step)
        .ceil()
        .as_ivec2()
        .min(extent.as_ivec2())
        .min(clip.max);
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

/// Paints `paint` into its terrain chunk image: each texel under the paint's
/// square takes the nearest paint texel, blended by twice its alpha (the
/// `MODULATE2X` the road decal uses).
pub(super) fn blend_terrain_surface_paint_into_terrain_surface_image(
    surface: &mut Image,
    clip: IRect,
    chunk: &TerrainChunk,
    margin_cells: f32,
    paint: &TerrainPaintedTexture,
    texture: &Image,
) -> Option<()> {
    let chunk_cells = f32::from(chunk.side.checked_sub(1)?);
    let extent = read_rgba_image_extent(surface)?;
    let texture_extent = read_rgba_image_extent(texture)?;
    let output_origin = chunk.origin - Vec2::splat(margin_cells * chunk.spacing_m);
    let output_texel_step =
        margin_cells.mul_add(2.0, chunk_cells) * chunk.spacing_m / extent.as_vec2();
    if output_texel_step.cmple(Vec2::ZERO).any() || paint.size_m <= 0.0 {
        return None;
    }
    let paint_min = paint.centre - Vec2::splat(paint.size_m * 0.5);
    let destination_min = ((paint_min - output_origin) / output_texel_step)
        .floor()
        .as_ivec2()
        .max(clip.min)
        .as_uvec2();
    let destination_max = ((paint_min + Vec2::splat(paint.size_m) - output_origin)
        / output_texel_step)
        .ceil()
        .as_ivec2()
        .clamp(IVec2::ZERO, extent.as_ivec2())
        .min(clip.max)
        .as_uvec2();
    let surface_bytes = surface.data.as_mut()?;
    for destination_y in destination_min.y..destination_max.y {
        for destination_x in destination_min.x..destination_max.x {
            let world = output_origin
                + (UVec2::new(destination_x, destination_y).as_vec2() + 0.5) * output_texel_step;
            // V runs north (-Z), as the original lays a painted texture.
            let local = Vec2::new(world.x - paint_min.x, paint_min.y + paint.size_m - world.y)
                / paint.size_m;
            if local.cmplt(Vec2::ZERO).any() || local.cmpge(Vec2::ONE).any() {
                continue;
            }
            let texel = ((paint.uv_min + (paint.uv_max - paint.uv_min) * local)
                * texture_extent.as_vec2())
            .floor()
            .as_ivec2()
            .rem_euclid(texture_extent.as_ivec2())
            .as_uvec2();
            let source = read_wrapped_rgba_image_pixel(texture, texel.x, texel.y)?;
            let alpha = (u16::from(source[3]) * 2).min(255);
            if alpha == 0 {
                continue;
            }
            let target = (destination_y * extent.x + destination_x) as usize * 4;
            for channel in 0..3 {
                let ground = u16::from(surface_bytes[target + channel]);
                let blended = (u16::from(source[channel]) * alpha + ground * (255 - alpha)) / 255;
                surface_bytes[target + channel] = u8::try_from(blended).unwrap_or(u8::MAX);
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod painted_texture_tests {
    use super::*;

    fn image(width: u32, pixels: &[[u8; 4]]) -> Image {
        Image::new(
            Extent3d {
                width,
                height: pixels.len() as u32 / width,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.concat(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn paint_covers_its_square_blended_by_twice_its_alpha() {
        // A 4 m chunk drawn 1 texel per metre, no margin.
        let chunk = TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::ZERO,
            asset: Handle::default(),
            origin: Vec2::ZERO,
            spacing_m: 1.0,
            side: 5,
        };
        let mut surface = image(4, &[[0, 0, 0, 255]; 16]);
        // Left half opaque white, right half alpha 64 (blends at 128).
        let texture = image(2, &[[255, 255, 255, 128], [255, 255, 255, 64]]);
        let paint = TerrainPaintedTexture {
            centre: Vec2::new(2.0, 2.0),
            size_m: 2.0,
            texture: Handle::default(),
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
        };
        blend_terrain_surface_paint_into_terrain_surface_image(
            &mut surface,
            IRect::from_corners(IVec2::ZERO, IVec2::splat(4)),
            &chunk,
            0.0,
            &paint,
            &texture,
        )
        .unwrap();
        let red = |x: usize, y: usize| surface.data.as_ref().unwrap()[(y * 4 + x) * 4];
        assert_eq!(
            [red(0, 1), red(1, 1), red(2, 1), red(3, 1)],
            [0, 255, 128, 0]
        );
        assert_eq!([red(1, 0), red(1, 3)], [0, 0]);
    }
}

#[cfg(test)]
mod region_recomposition_tests {
    use super::*;
    use openzt2_game_assets::terrain::terrain_test_asset::{
        terrain_test_asset, terrain_test_textures_mut,
    };
    use openzt2_game_data::terrain::{
        TerrainGrid, TerrainPresentation, TerrainSample, TerrainSlopeTile, TerrainUnits,
    };
    use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::materialize_complete_edited_terrain_sample_overlay;

    fn rgba_image(side: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Image {
        let data = (0..side * side)
            .flat_map(|index| pixel(index % side, index / side))
            .collect();
        Image::new(
            Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD,
        )
    }

    fn biome(texture: &str, draw_order: i16) -> TerrainBiome {
        TerrainBiome {
            name: String::new(),
            ground_texture: texture.into(),
            cover_texture: texture.into(),
            mix_mask: String::new(),
            cliff_textures: Default::default(),
            ground_brushes: std::array::from_fn(|_| "blend.dds".into()),
            cover_brushes: std::array::from_fn(|_| "blend.dds".into()),
            ground_no_blend_brushes: std::array::from_fn(|_| "solid.dds".into()),
            cover_no_blend_brushes: std::array::from_fn(|_| "solid.dds".into()),
            draw_order,
            shallow_water_depth_mm: None,
            deep_water_depth_mm: None,
            water_shore_offset_mm: None,
            water_presentation: None,
        }
    }

    #[test]
    fn recomposing_the_changed_region_matches_composing_the_whole_chunk() {
        let mut images = Assets::<Image>::default();
        let mut asset = terrain_test_asset(TerrainGrid {
            width: 17,
            height: 17,
            sector_columns: 1,
            sector_rows: 1,
            units: TerrainUnits {
                cell_size_metres: 1.0,
                base_height_metres: 0.0,
                maximum_height_metres: f32::MAX,
                minimum_water_height_metres: None,
            },
            presentation: TerrainPresentation {
                water_distance_fade: Default::default(),
                water_editing: Default::default(),
                detail_texture: String::new(),
                detail_texture_repetition: 1.0,
                cliff_rise_per_metre: [0.0; 2],
                surface_resolution: 128,
                surface_margin_cells: 1,
                full_alpha_composition: true,
            },
            biomes: vec![biome("red.dds", 0), biome("blue.dds", 1)],
            samples: (0..17 * 17)
                .map(|_| TerrainSample {
                    height_relative_to_base_metres: 0.0,
                    biome_index: 0,
                    has_ground_cover: false,
                    water_depth: TerrainWaterDepth::Dry,
                    water_surface_flag: false,
                    excludes_elevated_path: false,
                    support_floor_metres: None,
                    height_cap_metres: None,
                    height_is_linked: false,
                    has_child_height_link: false,
                })
                .collect(),
            water_regions: Vec::new(),
            waterfall: None,
            slope_tiles: vec![
                TerrainSlopeTile {
                    uses_alternate_diagonal: false
                };
                16
            ],
        });
        let textures = terrain_test_textures_mut(&mut asset);
        // A soft round brush, so neighbouring stamps overlap and blend.
        let round = |x: u32, y: u32| {
            let distance = (x as f32 - 7.5).hypot(y as f32 - 7.5);
            let value = (255.0 * (1.0 - distance / 9.0)).clamp(0.0, 255.0) as u8;
            [value, value, value, 255]
        };
        textures.insert("blend.dds".into(), images.add(rgba_image(16, round)));
        textures.insert(
            "solid.dds".into(),
            images.add(rgba_image(16, |_, _| [255; 4])),
        );
        textures.insert(
            "red.dds".into(),
            images.add(rgba_image(32, |x, y| {
                [200, (x * 7) as u8, (y * 5) as u8, 255]
            })),
        );
        textures.insert(
            "blue.dds".into(),
            images.add(rgba_image(32, |x, y| {
                [(x * 3) as u8, (y * 6) as u8, 220, 255]
            })),
        );
        let chunk = TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::new(0, -1),
            asset: Handle::default(),
            origin: Vec2::ZERO,
            spacing_m: 1.0,
            side: 17,
        };
        let mut edited = materialize_complete_edited_terrain_sample_overlay(&chunk, &asset);
        let before = compose_terrain_surface_image(
            &chunk,
            &asset,
            &[(chunk.asset.clone(), chunk.coord, edited.clone())],
            &images,
        )
        .expect("unedited surface");
        let (min, max) = (UVec2::new(5, 6), UVec2::new(7, 8));
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                let sample = (z * 17 + x) as usize;
                edited.blend_weights[sample * 16..sample * 16 + 16].fill(0);
                edited.blend_weights[sample * 16 + 1] = 255;
            }
        }
        let edited_chunks = [(chunk.asset.clone(), chunk.coord, edited)];
        let whole = compose_terrain_surface_image(&chunk, &asset, &edited_chunks, &images)
            .expect("whole surface");
        let region = recompose_terrain_surface_image_region(
            &chunk,
            &asset,
            &edited_chunks,
            &images,
            &before,
            Some((min, max)),
            None,
        )
        .expect("region surface");
        assert_ne!(before.data, whole.data, "the edit changes the surface");
        assert_eq!(region.0.data, whole.data);
    }
}

use crate::terrain::ground_path_surface_mask_selection::ground_path_surface_origin_from_source_direction_cell;
use crate::terrain::ground_path_surface_mask_selection::select_authored_ground_path_surface_mask_textures;
use bevy::asset::AssetId as BevyAssetId;
use bevy::ecs::query::QueryData;
use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::ground_path_surface_images::GroundPathSurfaceImage;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout;
use openzt2_zoo_state::topology::ground_path_boundary_queries::ground_path_tile_triangles;
use openzt2_zoo_state::topology::ground_path_boundary_queries::ground_path_triangle_curb_boundary;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_reciprocal_fence_curve_partners;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceSegmentShape;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::terrain_change_tracking_types::TerrainSurfaceBasePending;
use crate::terrain::terrain_surface_image_composition::blend_ground_path_surface_patch_into_terrain_surface_image;
use crate::terrain::terrain_surface_image_composition::blend_terrain_surface_paint_into_terrain_surface_image;
use crate::terrain::terrain_surface_image_composition::{
    compose_terrain_surface_image, recompose_terrain_surface_image_region,
};
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainPaintedTexture;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainSurfaceImage;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

#[derive(Message)]
pub(super) struct DiscardedTerrainPaintArea(Rect);

#[derive(Component)]
pub(super) struct GroundPathSurfaceProjectionPending;

#[derive(Clone, Copy)]
struct GroundPathSurfaceOverlay {
    cell: IVec3,
    texture: BevyAssetId<Image>,
    mask: BevyAssetId<Image>,
    mirror_x: bool,
    mirror_y: bool,
}

/// Invalidates only terrain chunks whose composed image can contain a changed
/// path surface. Source reloads also invalidate chunks near existing paths.
#[allow(clippy::too_many_arguments)]
pub(super) fn mark_terrain_chunks_requiring_ground_path_surface_recomposition(
    mut commands: Commands,
    mut topology_changed: MessageReader<TopologyChanged>,
    added_paths: Query<&PathTile, Added<PathTile>>,
    mut image_events: MessageReader<AssetEvent<GroundPathSurfaceImage>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    paths: Query<&PathTile>,
    chunks: Query<(Entity, &TerrainChunk), With<TerrainRenderChunk>>,
) {
    let dependency_image_changed = image_events.read().any(|event| {
        matches!(*event, AssetEvent::LoadedWithDependencies { id }
            if active_definitions.owns_ground_path_surface_image(id))
    });
    image_events.clear();
    let source_changed = dependency_image_changed || active_definitions.is_changed();

    let changed_bounds = topology_changed
        .read()
        .map(|change| change.bounds)
        .chain(
            added_paths
                .iter()
                .map(|path| IRect::from_corners(path.cell.xy(), path.cell.xy())),
        )
        .reduce(|current, next| current.union(next));
    if changed_bounds.is_none() && !source_changed {
        return;
    }
    let surface_patch_margin = Vec2::splat(grid.spacing_m * 5.0);
    let changed_world_bounds = changed_bounds.map(|bounds| {
        (
            grid.cell_translation(bounds.min.extend(0)).xz() - surface_patch_margin,
            grid.cell_translation(bounds.max.extend(0)).xz() + surface_patch_margin,
        )
    });
    for (entity, chunk) in &chunks {
        let chunk_cells = f32::from(chunk.side.saturating_sub(1));
        let chunk_min = chunk.origin;
        let chunk_max = chunk.origin + Vec2::splat(chunk_cells * chunk.spacing_m);
        let topology_changed = changed_world_bounds.is_some_and(|(world_min, world_max)| {
            chunk_min.cmple(world_max).all() && chunk_max.cmpge(world_min).all()
        });
        if topology_changed
            || (source_changed
                && paths.iter().any(|path| {
                    let path_origin = grid.cell_translation(path.cell).xz();
                    path_origin.cmple(chunk_max + surface_patch_margin).all()
                        && path_origin.cmpge(chunk_min - surface_patch_margin).all()
                }))
        {
            commands
                .entity(entity)
                .insert(GroundPathSurfaceProjectionPending);
        }
    }
}

pub(super) fn record_discarded_terrain_paint_area(
    discard: On<Discard<TerrainPaintedTexture>>,
    paints: Query<&TerrainPaintedTexture>,
    mut discarded: MessageWriter<DiscardedTerrainPaintArea>,
) {
    if let Ok(paint) = paints.get(discard.entity) {
        discarded.write(DiscardedTerrainPaintArea(terrain_paint_area(paint)));
    }
}

fn terrain_paint_area(paint: &TerrainPaintedTexture) -> Rect {
    Rect::from_center_size(paint.centre, Vec2::splat(paint.size_m))
}

/// Invalidates the terrain chunks under a painted texture that was added,
/// changed or removed (a road piece built or demolished).
pub(super) fn mark_terrain_chunks_under_changed_painted_textures(
    mut commands: Commands,
    changed: Query<&TerrainPaintedTexture, Changed<TerrainPaintedTexture>>,
    mut discarded: MessageReader<DiscardedTerrainPaintArea>,
    chunks: Query<(Entity, &TerrainChunk), With<TerrainRenderChunk>>,
) {
    let changed_areas = discarded
        .read()
        .map(|discarded| discarded.0)
        .chain(changed.iter().map(terrain_paint_area))
        .collect::<Vec<_>>();
    if changed_areas.is_empty() {
        return;
    }
    for (entity, chunk) in &chunks {
        // The chunk image reaches a few cells past the chunk.
        let chunk_area = Rect::from_corners(
            chunk.origin,
            chunk.origin + Vec2::splat(f32::from(chunk.side.saturating_sub(1)) * chunk.spacing_m),
        )
        .inflate(chunk.spacing_m * 5.0);
        if changed_areas
            .iter()
            .any(|area| !area.intersect(chunk_area).is_empty())
        {
            commands
                .entity(entity)
                .insert(GroundPathSurfaceProjectionPending);
        }
    }
}

#[derive(QueryData)]
pub(super) struct GroundPathSurfaceChunk {
    chunk: &'static TerrainChunk,
    edited: Option<&'static EditedTerrainSamples>,
    surface: &'static TerrainSurfaceImage,
    base: Option<&'static TerrainSurfaceBasePending>,
}

/// Rebuilds a terrain chunk's base image and applies the
/// ground-path surface-patch records directly to that owned image.
#[allow(clippy::too_many_arguments)]
pub(super) fn recompose_ground_path_surfaces_into_terrain_chunk_images(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    grid: Res<TopologyGrid>,
    paths: Query<(Entity, &PathTile)>,
    paints: Query<&TerrainPaintedTexture>,
    chunks: Query<GroundPathSurfaceChunk, With<TerrainRenderChunk>>,
    terrain_index: Res<TerrainIndex>,
    added_chunks: Query<Entity, (With<TerrainSurfaceImage>, Added<TerrainRenderChunk>)>,
    base_pending_chunks: Query<Entity, With<TerrainSurfaceBasePending>>,
    pending_chunks: Query<
        Entity,
        (
            With<TerrainSurfaceImage>,
            With<GroundPathSurfaceProjectionPending>,
        ),
    >,
    surface_images: Res<Assets<GroundPathSurfaceImage>>,
    server: Res<AssetServer>,
) {
    if added_chunks.is_empty() && pending_chunks.is_empty() && base_pending_chunks.is_empty() {
        return;
    }
    let mut retain_changed_chunks = || {
        for entity in &added_chunks {
            if !pending_chunks.contains(entity) {
                commands
                    .entity(entity)
                    .insert(GroundPathSurfaceProjectionPending);
            }
        }
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        retain_changed_chunks();
        return;
    };
    let source_grid_offset = (Vec2::new(grid.origin.x, -grid.origin.y) / grid.spacing_m)
        .round()
        .as_ivec2()
        .extend(0);
    let path_layouts = paths
        .iter()
        .filter_map(|(_, path)| {
            let definition = definitions.find_path(path.definition)?;
            if definition.elevated {
                return None;
            }
            let cell = source_grid_offset + IVec3::new(path.cell.x, -path.cell.y, path.cell.z);
            let (centre, width, _) = calculate_snapped_ground_path_layout(
                cell,
                definition.width_cm,
                definitions.topology_cell_size_cm(),
            )?;
            Some((AssetId(definition.curb.0), cell, centre, width))
        })
        .collect::<Vec<_>>();
    let triangles = path_layouts
        .iter()
        .flat_map(|&(_, cell, centre, width)| ground_path_tile_triangles(cell, centre, width))
        .collect::<HashSet<_>>();
    if path_layouts
        .iter()
        .any(|&(curb, ..)| curb != AssetId::default() && definitions.find_fence(curb).is_none())
    {
        retain_changed_chunks();
        return;
    }
    let mut segments = path_layouts
        .iter()
        .flat_map(|&(curb, cell, centre, width)| {
            let triangles = &triangles;
            ground_path_tile_triangles(cell, centre, width).flat_map(move |triangle| {
                ground_path_triangle_curb_boundary(triangles, centre, triangle)
                    .map(move |(a, b)| (curb, a, b))
            })
        })
        .collect::<Vec<_>>();
    // Duplicate path records must not turn one corner into a junction.
    let mut seen = HashSet::new();
    segments.retain(|&(curb, a, b)| EdgeKey::new(a, b).is_some_and(|key| seen.insert((curb, key))));
    let partners = select_reciprocal_fence_curve_partners(&segments);
    let curb_edges = segments
        .iter()
        .zip(partners)
        .filter_map(|(&(curb, a, b), partner)| {
            let (outer, corner, next) = partner?;
            let definition = definitions.find_fence(curb)?;
            let selected = select_authored_fence_segment_prefab_for_adjacent_topology(
                definition,
                outer,
                corner,
                Some(next),
            );
            (selected.shape != FenceSegmentShape::Straight).then(|| {
                (
                    (curb, EdgeKey::new(a, b).expect("validated curb edge")),
                    (outer, corner, next),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    // Resolve each source once per composition pass. The path asset owns the handles.
    let mut resolved_images = HashMap::new();
    let mut resolve_image = |identifier| {
        *resolved_images.entry(identifier).or_insert_with(|| {
            let source = active_definitions.ground_path_surface_image(identifier)?;
            if !server.is_loaded_with_dependencies(source.id()) {
                return None;
            }
            Some(surface_images.get(source)?.image.id())
        })
    };
    let overlays = paths
        .iter()
        .filter_map(|(entity, path)| {
            let definition = definitions.find_path(path.definition)?;
            if definition.elevated {
                return None;
            }
            // The mask switches use source XY, while topology owns
            // Bevy XZ. Convert only this transient presentation calculation.
            let source_cell =
                source_grid_offset + IVec3::new(path.cell.x, -path.cell.y, path.cell.z);
            let (centre, width_cells, _) = calculate_snapped_ground_path_layout(
                source_cell,
                definition.width_cm,
                definitions.topology_cell_size_cm(),
            )?;
            let direction_offset = width_cells / 2;
            let relative = source_cell - centre;
            let direction = match relative.xy() {
                offset if offset == IVec2::new(0, direction_offset) => 0,
                offset if offset == IVec2::new(direction_offset, 0) => 2,
                offset if offset == IVec2::new(0, -direction_offset) => 4,
                offset if offset == IVec2::new(-direction_offset, 0) => 6,
                _ => return None,
            };
            let curb = AssetId(definition.curb.0);
            let Some(mut mask_texture_selections) =
                select_authored_ground_path_surface_mask_textures(
                    direction,
                    centre,
                    width_cells,
                    curb,
                    &curb_edges,
                )
            else {
                error!(
                    path = ?path.definition,
                    cell = ?path.cell,
                    direction,
                    "ground-path direction is outside the four authored cardinal directions"
                );
                return None;
            };
            for selection in mask_texture_selections.iter_mut().flatten() {
                selection.cell = ground_path_surface_origin_from_source_direction_cell(
                    selection.cell,
                    definition.width_cm,
                    definitions.topology_cell_size_cm(),
                )? + IVec3::new(-source_grid_offset.x, source_grid_offset.y, 0);
            }
            Some((entity, definition.surface_texture, mask_texture_selections))
        })
        .flat_map(|(entity, texture, selections)| {
            selections
                .into_iter()
                .flatten()
                .map(move |selection| (entity, texture, selection))
        })
        .map(|(entity, texture, selection)| {
            Some((
                entity,
                GroundPathSurfaceOverlay {
                    cell: selection.cell,
                    texture: resolve_image(texture)?,
                    mask: resolve_image(AssetId::from_virtual_path(selection.source_path))?,
                    mirror_x: selection.mirror_mode & 1 != 0,
                    mirror_y: selection.mirror_mode & 2 != 0,
                },
            ))
        })
        .collect::<Option<Vec<_>>>();
    // An unavailable source only blocks composition if a selected patch uses it.
    let Some(mut overlays) = overlays else {
        retain_changed_chunks();
        return;
    };
    overlays.sort_unstable_by_key(|(entity, overlay)| {
        (-overlay.cell.y, overlay.cell.x, entity.to_bits())
    });

    for entity in
        pending_chunks
            .iter()
            .chain(
                added_chunks
                    .iter()
                    .filter(|entity| !pending_chunks.contains(*entity)),
            )
            .chain(base_pending_chunks.iter().filter(|entity| {
                !added_chunks.contains(*entity) && !pending_chunks.contains(*entity)
            }))
    {
        let Ok(GroundPathSurfaceChunkItem {
            chunk,
            surface,
            base,
            ..
        }) = chunks.get(entity)
        else {
            continue;
        };
        let dirty = pending_chunks.contains(entity);
        let mut retain_chunk = || {
            if !dirty {
                commands
                    .entity(entity)
                    .insert(GroundPathSurfaceProjectionPending);
            }
        };
        let chunk_cells = f32::from(chunk.side.saturating_sub(1));
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            retain_chunk();
            continue;
        };
        let margin_cells = f32::from(
            asset
                .canonical_terrain_grid()
                .presentation
                .surface_margin_cells,
        );
        let image_min = chunk.origin - Vec2::splat(margin_cells * chunk.spacing_m);
        let image_max = chunk.origin + Vec2::splat((chunk_cells + margin_cells) * chunk.spacing_m);
        let relevant_overlays = overlays
            .iter()
            .map(|(_, overlay)| *overlay)
            .filter(|overlay| {
                let origin = grid.cell_translation(overlay.cell).xz();
                let native_patch_extent = chunk_cells * chunk.spacing_m * 43.0
                    / f32::from(
                        asset
                            .canonical_terrain_grid()
                            .presentation
                            .surface_resolution,
                    );
                let minimum = origin - Vec2::new(0.0, native_patch_extent);
                let maximum = origin + Vec2::new(native_patch_extent, 0.0);
                minimum.cmple(image_max).all() && maximum.cmpge(image_min).all()
            })
            .collect::<Vec<_>>();
        let relevant_paints = paints
            .iter()
            .filter(|paint| {
                (paint.centre - (image_min + image_max) * 0.5)
                    .abs()
                    .cmple((image_max - image_min + Vec2::splat(paint.size_m)) * 0.5)
                    .all()
            })
            .map(|paint| Some((paint, images.get(&paint.texture)?)))
            .collect::<Option<Vec<_>>>();
        // Wait for a painted texture to load rather than drop it.
        let Some(relevant_paints) = relevant_paints else {
            retain_chunk();
            continue;
        };
        if relevant_overlays.is_empty() && relevant_paints.is_empty() && base.is_none() && !dirty {
            continue;
        }
        let Some(previous) = images.get(&surface.0) else {
            retain_chunk();
            continue;
        };
        let full = IRect::from_corners(
            IVec2::ZERO,
            IVec2::new(
                previous.texture_descriptor.size.width as i32,
                previous.texture_descriptor.size.height as i32,
            ),
        );
        let (mut composed, clip) = if dirty || base.is_some() {
            let indexed_neighbours = &terrain_index.chunks;
            let neighbours = (-1..=1).flat_map(|z| {
                (-1..=1).filter_map(move |x| {
                    indexed_neighbours
                        .get(&(chunk.coord + IVec2::new(x, z)))
                        .copied()
                })
            });
            let edited_chunks = chunks
                .iter_many(neighbours)
                .filter(|candidate| candidate.chunk.asset == chunk.asset)
                .filter_map(|candidate| {
                    candidate.edited.map(|edited| {
                        (
                            candidate.chunk.asset.clone(),
                            candidate.chunk.coord,
                            edited.clone(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            let next = if let Some(base) = base.filter(|_| !dirty) {
                recompose_terrain_surface_image_region(
                    chunk,
                    asset,
                    &edited_chunks,
                    &images,
                    previous,
                    base.0.min,
                    base.0.max,
                )
            } else {
                compose_terrain_surface_image(chunk, asset, &edited_chunks, &images)
                    .map(|image| (image, full))
            };
            let Some(next) = next else {
                continue;
            };
            next
        } else {
            (previous.clone(), full)
        };
        for overlay in relevant_overlays {
            let Some((path_texture, detail_mask)) =
                images.get(overlay.texture).zip(images.get(overlay.mask))
            else {
                continue;
            };
            if blend_ground_path_surface_patch_into_terrain_surface_image(
                &mut composed,
                clip,
                chunk,
                asset,
                grid.cell_translation(overlay.cell).xz(),
                path_texture,
                detail_mask,
                overlay.mirror_x,
                overlay.mirror_y,
            )
            .is_none()
            {
                error!(texture = ?overlay.texture, mask = ?overlay.mask,
                    path_format = ?path_texture.texture_descriptor.format,
                    mask_format = ?detail_mask.texture_descriptor.format,
                    "ground path surface blend rejected source image");
            }
        }
        for (paint, texture) in relevant_paints {
            if blend_terrain_surface_paint_into_terrain_surface_image(
                &mut composed,
                clip,
                chunk,
                margin_cells,
                paint,
                texture,
            )
            .is_none()
            {
                error!(texture_format = ?texture.texture_descriptor.format,
                    "terrain painted texture rejected source image");
            }
        }
        let Some(mut image) = images.get_mut(&surface.0) else {
            retain_chunk();
            continue;
        };
        *image = composed;
        if base.is_some() {
            commands
                .entity(entity)
                .remove::<TerrainSurfaceBasePending>();
        }
        if dirty {
            commands
                .entity(entity)
                .remove::<GroundPathSurfaceProjectionPending>();
        }
    }
}

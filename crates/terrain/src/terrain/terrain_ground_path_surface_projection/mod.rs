use crate::terrain::ground_path_surface_mask_selection::ground_path_surface_origin_from_source_direction_cell;
use crate::terrain::ground_path_surface_mask_selection::select_authored_ground_path_surface_mask_textures;
use bevy::asset::AssetId as BevyAssetId;
use bevy::ecs::query::QueryData;
use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::tasks::{ComputeTaskPool, TaskPool};
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
use openzt2_zoo_state::topology::topology_graph_types::{TopologyGrid, TopologyIndex};

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
pub(super) struct RecomposeTerrainOverlays {
    bounds: Option<Rect>,
    paths_changed: bool,
}

impl RecomposeTerrainOverlays {
    fn merge(&mut self, other: Self) {
        self.bounds = self
            .bounds
            .zip(other.bounds)
            .map(|(first, next)| first.union(next));
        self.paths_changed |= other.paths_changed;
    }
}

#[derive(Message)]
pub(super) struct TerrainOverlayInvalidated(Entity, RecomposeTerrainOverlays);

#[derive(Component)]
pub(super) struct TerrainChunkOverlays {
    paths: Vec<GroundPathSurfaceOverlay>,
}

#[derive(Component)]
#[relationship(relationship_target = TerrainPaintChunkInputs)]
pub(super) struct TerrainPaintChunk(Entity);

#[derive(Component)]
#[relationship_target(relationship = TerrainPaintChunk, linked_spawn)]
pub(super) struct TerrainPaintChunkInputs(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = TerrainPaintAffectedChunks)]
pub(super) struct TerrainPaintInput(Entity);

#[derive(Component)]
#[relationship_target(relationship = TerrainPaintInput, linked_spawn)]
pub(super) struct TerrainPaintAffectedChunks(Vec<Entity>);

#[derive(Component)]
pub(super) struct TerrainPaintInputOrder(usize);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct TerrainPaintLinks<'w, 's> {
    chunks: Query<'w, 's, &'static TerrainPaintChunkInputs>,
    edges: Query<
        'w,
        's,
        (
            Entity,
            &'static TerrainPaintInput,
            &'static TerrainPaintInputOrder,
        ),
    >,
}

impl TerrainPaintLinks<'_, '_> {
    fn for_chunk(&self, chunk: Entity) -> impl Iterator<Item = (Entity, Entity, usize)> + '_ {
        self.edges
            .iter_many(
                self.chunks
                    .get(chunk)
                    .ok()
                    .into_iter()
                    .flat_map(RelationshipTarget::iter),
            )
            .flatten()
            .map(|(edge, input, order)| (edge, input.0, order.0))
    }
}

#[derive(Component)]
pub(super) struct PreparedTerrainChunkImage(Option<Image>);

#[derive(Clone, Copy)]
struct GroundPathSurfaceOverlay {
    cell: IVec3,
    origin: Vec2,
    texture: BevyAssetId<Image>,
    mask: BevyAssetId<Image>,
    mirror_x: bool,
    mirror_y: bool,
}

/// Invalidates only terrain chunks whose composed image can contain a changed
/// path surface. Source reloads also invalidate chunks near existing paths.
#[allow(clippy::too_many_arguments)]
pub(super) fn mark_terrain_chunks_requiring_ground_path_surface_recomposition(
    mut dirty: MessageWriter<TerrainOverlayInvalidated>,
    mut topology_changed: MessageReader<TopologyChanged>,
    added_paths: Query<&PathTile, Changed<PathTile>>,
    mut discarded_paths: MessageReader<DiscardedGroundPathCell>,
    mut image_events: MessageReader<AssetEvent<GroundPathSurfaceImage>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    paths: Query<&PathTile>,
    chunks: Query<(Entity, &TerrainChunk), With<TerrainRenderChunk>>,
) {
    let dependency_image_changed = image_events.read().any(|event| match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Removed { id } => active_definitions.owns_ground_path_surface_image(id),
        AssetEvent::Unused { .. } => false,
    });
    image_events.clear();
    let source_changed = dependency_image_changed || active_definitions.is_changed();

    let changed_bounds = topology_changed
        .read()
        .map(|change| change.bounds)
        .chain(
            discarded_paths
                .read()
                .map(|cell| IRect::from_corners(cell.0, cell.0)),
        )
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
        let near_path = source_changed
            && paths.iter().any(|path| {
                let path_origin = grid.cell_translation(path.cell).xz();
                path_origin.cmple(chunk_max + surface_patch_margin).all()
                    && path_origin.cmpge(chunk_min - surface_patch_margin).all()
            });
        if topology_changed || near_path {
            dirty.write(TerrainOverlayInvalidated(
                entity,
                RecomposeTerrainOverlays {
                    bounds: if near_path {
                        None
                    } else {
                        changed_world_bounds.map(|(min, max)| Rect::from_corners(min, max))
                    },
                    paths_changed: true,
                },
            ));
        }
    }
}

#[derive(Message)]
pub(super) struct DiscardedGroundPathCell(IVec2);

pub(super) fn record_discarded_ground_path_cell(
    discard: On<Discard<PathTile>>,
    paths: Query<&PathTile>,
    mut discarded: MessageWriter<DiscardedGroundPathCell>,
) {
    if let Ok(path) = paths.get(discard.entity) {
        discarded.write(DiscardedGroundPathCell(path.cell.xy()));
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
    mut dirty: MessageWriter<TerrainOverlayInvalidated>,
    changed: Query<&TerrainPaintedTexture, Changed<TerrainPaintedTexture>>,
    mut discarded: MessageReader<DiscardedTerrainPaintArea>,
    chunks: Query<(Entity, &TerrainChunk, Option<&TerrainChunkOverlays>), With<TerrainRenderChunk>>,
    assets: Res<Assets<TerrainAsset>>,
    paints: Query<&TerrainPaintedTexture>,
    paint_links: TerrainPaintLinks,
    mut image_events: MessageReader<AssetEvent<Image>>,
) {
    let image_changes: HashSet<_> = image_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect();
    let changed_areas = discarded
        .read()
        .map(|discarded| discarded.0)
        .chain(changed.iter().map(terrain_paint_area))
        .collect::<Vec<_>>();
    if changed_areas.is_empty() && image_changes.is_empty() {
        return;
    }
    for (entity, chunk, overlays) in &chunks {
        let Some(asset) = assets.get(&chunk.asset) else {
            continue;
        };
        let margin = f32::from(
            asset
                .canonical_terrain_grid()
                .presentation
                .surface_margin_cells,
        );
        let chunk_area = Rect::from_corners(
            chunk.origin,
            chunk.origin + Vec2::splat(f32::from(chunk.side.saturating_sub(1)) * chunk.spacing_m),
        )
        .inflate(chunk.spacing_m * margin);
        let mut areas: Vec<_> = changed_areas
            .iter()
            .copied()
            .filter(|area| !area.intersect(chunk_area).is_empty())
            .collect();
        if let Some(overlays) = overlays {
            areas.extend(
                paints
                    .iter_many(paint_links.for_chunk(entity).map(|(_, paint, _)| paint))
                    .flatten()
                    .filter(|paint| image_changes.contains(&paint.texture.id()))
                    .map(terrain_paint_area),
            );
            let settings = &asset.canonical_terrain_grid().presentation;
            let extent = f32::from(chunk.side.saturating_sub(1)) * chunk.spacing_m * 43.0
                / f32::from(settings.surface_resolution);
            areas.extend(
                overlays
                    .paths
                    .iter()
                    .filter(|overlay| {
                        image_changes.contains(&overlay.texture)
                            || image_changes.contains(&overlay.mask)
                    })
                    .map(|overlay| {
                        Rect::from_corners(
                            overlay.origin - Vec2::new(0.0, extent),
                            overlay.origin + Vec2::new(extent, 0.0),
                        )
                    }),
            );
        }
        if let Some(bounds) = areas
            .into_iter()
            .reduce(|current, next| current.union(next))
        {
            dirty.write(TerrainOverlayInvalidated(
                entity,
                RecomposeTerrainOverlays {
                    bounds: Some(bounds),
                    paths_changed: false,
                },
            ));
        }
    }
}

pub(super) fn merge_terrain_overlay_invalidations(
    mut commands: Commands,
    mut changes: MessageReader<TerrainOverlayInvalidated>,
    mut pending: Query<&mut RecomposeTerrainOverlays>,
) {
    let mut dirty = bevy::ecs::entity::EntityHashMap::<RecomposeTerrainOverlays>::default();
    for TerrainOverlayInvalidated(entity, region) in changes.read() {
        let next = RecomposeTerrainOverlays {
            bounds: region.bounds,
            paths_changed: region.paths_changed,
        };
        if let Some(current) = dirty.get_mut(entity) {
            current.merge(next);
        } else {
            dirty.insert(*entity, next);
        }
    }
    for (entity, next) in dirty {
        if let Ok(mut current) = pending.get_mut(entity) {
            current.merge(next);
        } else {
            commands.entity(entity).try_insert(next);
        }
    }
}

#[derive(QueryData)]
pub(super) struct GroundPathSurfaceChunk {
    chunk: &'static TerrainChunk,
    edited: Option<&'static EditedTerrainSamples>,
    surface: &'static TerrainSurfaceImage,
    base: Option<&'static TerrainSurfaceBasePending>,
    pending: Option<&'static RecomposeTerrainOverlays>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_ground_path_patches(
    paths: &[(Entity, &PathTile)],
    grid: TopologyGrid,
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    active_definitions: &WorldDefinitions,
    surface_images: &Assets<GroundPathSurfaceImage>,
    server: &AssetServer,
) -> Option<Vec<GroundPathSurfaceOverlay>> {
    let source_grid_offset = (Vec2::new(grid.origin.x, -grid.origin.y) / grid.spacing_m)
        .round()
        .as_ivec2()
        .extend(0);
    let path_layouts = paths
        .iter()
        .copied()
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
        return None;
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
        .copied()
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
                    origin: grid.cell_translation(selection.cell).xz(),
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
        return None;
    };
    overlays.sort_unstable_by_key(|(entity, overlay)| {
        (-overlay.cell.y, overlay.cell.x, entity.to_bits())
    });

    Some(overlays.into_iter().map(|(_, overlay)| overlay).collect())
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn prepare_terrain_chunk_surface_images(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    grid: Res<TopologyGrid>,
    topology: Res<TopologyIndex>,
    paths: Query<(Entity, &PathTile)>,
    paints: Query<(Entity, &TerrainPaintedTexture)>,
    chunks: Query<GroundPathSurfaceChunk, With<TerrainRenderChunk>>,
    terrain_index: Res<TerrainIndex>,
    pending_chunks: Query<
        Entity,
        (
            With<TerrainSurfaceImage>,
            Or<(
                Added<TerrainRenderChunk>,
                With<TerrainSurfaceBasePending>,
                With<RecomposeTerrainOverlays>,
                Without<TerrainChunkOverlays>,
            )>,
        ),
    >,
    surface_images: Res<Assets<GroundPathSurfaceImage>>,
    server: Res<AssetServer>,
    mut overlays: Query<&mut TerrainChunkOverlays>,
    paint_links: TerrainPaintLinks,
) {
    if pending_chunks.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut jobs = Vec::new();
    for entity in &pending_chunks {
        let Ok(GroundPathSurfaceChunkItem {
            chunk,
            surface,
            base,
            pending,
            ..
        }) = chunks.get(entity)
        else {
            continue;
        };
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let Some(previous) = images.get(&surface.0) else {
            continue;
        };
        let old = overlays.get(entity).ok();
        let presentation = &asset.canonical_terrain_grid().presentation;
        let margin_cells = f32::from(presentation.surface_margin_cells);
        let chunk_cells = f32::from(chunk.side.saturating_sub(1));
        let native_patch_extent =
            chunk_cells * chunk.spacing_m * 43.0 / f32::from(presentation.surface_resolution);
        let image_area = Rect::from_corners(
            chunk.origin - Vec2::splat(margin_cells * chunk.spacing_m),
            chunk.origin + Vec2::splat((chunk_cells + margin_cells) * chunk.spacing_m),
        );
        let paths =
            if let Some(old) = old.filter(|_| !pending.is_some_and(|dirty| dirty.paths_changed)) {
                old.paths.clone()
            } else {
                // A curb can depend on its neighbour's reciprocal corner. Keep
                // enough surrounding path cells for two snapped layout widths.
                let maximum_width_m = definitions
                    .paths()
                    .filter(|path| !path.elevated)
                    .map(|path| f32::from(path.width_cm) * 0.01)
                    .fold(0.0_f32, f32::max);
                let layout = image_area
                    .inflate(2.0 * maximum_width_m + native_patch_extent + grid.spacing_m * 3.0);
                let min = ((layout.min - grid.origin) / grid.spacing_m)
                    .floor()
                    .as_ivec2();
                let max = ((layout.max - grid.origin) / grid.spacing_m)
                    .ceil()
                    .as_ivec2();
                let topology = &*topology;
                let cells = (min.y..=max.y).flat_map(|y| {
                    (min.x..=max.x)
                        .filter_map(move |x| topology.paths.get(&IVec3::new(x, y, 0)).copied())
                });
                let path_rows: Vec<_> = paths.iter_many(cells).flatten().collect();
                let Some(mut paths) = resolve_ground_path_patches(
                    &path_rows,
                    *grid,
                    definitions,
                    &active_definitions,
                    &surface_images,
                    &server,
                ) else {
                    continue;
                };
                paths.retain(|overlay| {
                    let min = overlay.origin - Vec2::new(0.0, native_patch_extent);
                    let max = overlay.origin + Vec2::new(native_patch_extent, 0.0);
                    min.cmple(image_area.max).all() && max.cmpge(image_area.min).all()
                });
                paths
            };
        let paint_entities: Vec<_> = if old.is_some() && pending.is_none() {
            let mut paint_entities: Vec<_> = paint_links
                .for_chunk(entity)
                .map(|(_, paint, order)| (paint, order))
                .collect();
            paint_entities.sort_unstable_by_key(|&(_, order)| order);
            paint_entities
        } else {
            paints
                .iter()
                .enumerate()
                .filter_map(|(order, (entity, paint))| {
                    (!terrain_paint_area(paint).intersect(image_area).is_empty())
                        .then_some((entity, order))
                })
                .collect()
        };
        let relevant_paints = paints
            .iter_many(paint_entities.iter().map(|&(paint, _)| paint))
            .map(|item| {
                let (_, paint) = item.ok()?;
                Some((paint, images.get(&paint.texture)?))
            })
            .collect::<Option<Vec<_>>>();
        let Some(relevant_paints) = relevant_paints else {
            continue;
        };
        if paths
            .iter()
            .any(|overlay| !images.contains(overlay.texture) || !images.contains(overlay.mask))
        {
            continue;
        }
        let next = TerrainChunkOverlays { paths };
        if base.is_none()
            && pending.is_none()
            && next.paths.is_empty()
            && relevant_paints.is_empty()
        {
            commands.entity(entity).insert(next);
            continue;
        }
        let neighbours = (-1..=1).flat_map(|z| {
            let index = &terrain_index.chunks;
            (-1..=1).filter_map(move |x| index.get(&(chunk.coord + IVec2::new(x, z))).copied())
        });
        let edited = chunks
            .iter_many(neighbours)
            .flatten()
            .filter(|candidate| candidate.chunk.asset == chunk.asset)
            .filter_map(|candidate| {
                let edited = candidate.edited?;
                Some((
                    candidate.chunk.asset.clone(),
                    candidate.chunk.coord,
                    edited.clone(),
                ))
            })
            .collect::<Vec<_>>();
        let extent = UVec2::new(
            previous.texture_descriptor.size.width,
            previous.texture_descriptor.size.height,
        );
        let full = IRect::from_corners(IVec2::ZERO, extent.as_ivec2());
        let pixels = pending.and_then(|dirty| dirty.bounds).map(|bounds| {
            let step = (image_area.max - image_area.min) / extent.as_vec2();
            let min = ((bounds.min - image_area.min) / step)
                .floor()
                .as_ivec2()
                .max(IVec2::ZERO);
            let max = ((bounds.max - image_area.min) / step)
                .ceil()
                .as_ivec2()
                .min(extent.as_ivec2());
            IRect::from_corners(min.min(max), max)
        });
        let full_repaint = pending.is_some_and(|pending| pending.bounds.is_none()) || old.is_none();
        let samples = base.map(|base| (base.0.min, base.0.max));
        jobs.push((
            entity,
            chunk,
            asset,
            previous,
            full,
            margin_cells,
            edited,
            next,
            paint_entities,
            relevant_paints,
            full_repaint,
            samples,
            pixels,
        ));
    }
    let images = &*images;
    let prepared: Vec<_> = ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        for (entity, chunk, asset, previous, full, margin_cells, edited, overlays, paint_entities, paints,
            full_repaint, samples, pixels) in jobs {
            scope.spawn(async move {
                let restored = if full_repaint {
                    compose_terrain_surface_image(chunk, asset, &edited, images)
                        .map(|image| (image, full))
                } else {
                    recompose_terrain_surface_image_region(
                        chunk, asset, &edited, images, previous, samples, pixels,
                    )
                };
                let (mut composed, clip) = restored?;
                for overlay in &overlays.paths {
                    let (Some(texture), Some(mask)) = (images.get(overlay.texture), images.get(overlay.mask)) else { continue };
                    if blend_ground_path_surface_patch_into_terrain_surface_image(
                        &mut composed, clip, chunk, asset, overlay.origin, texture, mask,
                        overlay.mirror_x, overlay.mirror_y,
                    ).is_none() {
                        error!(texture = ?overlay.texture, mask = ?overlay.mask, "ground path surface blend rejected source image");
                    }
                }
                for (paint, texture) in paints {
                    if blend_terrain_surface_paint_into_terrain_surface_image(
                        &mut composed, clip, chunk, margin_cells, paint, texture,
                    ).is_none() {
                        error!(texture_format = ?texture.texture_descriptor.format,
                            "terrain painted texture rejected source image");
                    }
                }
                Some((entity, overlays, paint_entities, composed))
            });
        }
    });
    for (entity, next, paint_entities, image) in prepared.into_iter().flatten() {
        let mut existing: bevy::ecs::entity::EntityHashMap<(Entity, usize)> = paint_links
            .for_chunk(entity)
            .map(|(edge, paint, order)| (paint, (edge, order)))
            .collect();
        for (paint, order) in paint_entities {
            if let Some((edge, previous)) = existing.remove(&paint) {
                if previous == order {
                    continue;
                }
                commands.entity(edge).try_despawn();
            }
            commands.spawn((
                TerrainPaintChunk(entity),
                TerrainPaintInput(paint),
                TerrainPaintInputOrder(order),
            ));
        }
        for &(edge, _) in existing.values() {
            commands.entity(edge).try_despawn();
        }
        if let Ok(mut current) = overlays.get_mut(entity) {
            *current = next;
        } else {
            commands.entity(entity).insert(next);
        }
        commands
            .entity(entity)
            .insert(PreparedTerrainChunkImage(Some(image)));
    }
}

pub(super) fn publish_terrain_chunk_surface_images(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut prepared: Query<(Entity, &TerrainSurfaceImage, &mut PreparedTerrainChunkImage)>,
) {
    for (entity, surface, mut prepared) in &mut prepared {
        let Some(mut image) = images.get_mut(&surface.0) else {
            continue;
        };
        let Some(next) = prepared.0.take() else {
            continue;
        };
        *image = next;
        commands.entity(entity).remove::<(
            PreparedTerrainChunkImage,
            RecomposeTerrainOverlays,
            TerrainSurfaceBasePending,
        )>();
    }
}

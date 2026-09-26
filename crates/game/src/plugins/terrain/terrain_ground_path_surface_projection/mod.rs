use super::ground_path_surface_mask_selection::{
    ground_path_surface_origin_from_source_direction_cell,
    select_authored_ground_path_surface_mask_textures,
};
use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_data::AssetId;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout;
use crate::plugins::topology::topology_edit_types::TopologyChanged;
use crate::plugins::topology::topology_graph_types::EdgeKey;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::topology::topology_graph_types::PathTile;
use crate::plugins::topology::topology_graph_types::TopologyGrid;
use crate::plugins::topology::topology_graph_types::TopologyNode;

use super::{
    terrain_chunk_presentation_types::{TerrainRenderRevision, TerrainSurfaceImage},
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_surface_image_composition::{
        blend_ground_path_surface_patch_into_terrain_surface_image, compose_terrain_surface_image,
    },
};

#[derive(Resource, Default)]
pub(super) struct GroundPathSurfaceImageDependencies {
    images: HashMap<AssetId, Handle<Image>>,
    used_by_projected_paths: HashSet<AssetId>,
    definition_assets_containing_ground_path_surfaces:
        HashSet<bevy::asset::AssetId<WorldDefinitionAsset>>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GroundPathSurfaceProjectionRevision(u64);

#[derive(Component)]
pub(super) struct GroundPathSurfaceProjectionDirty;

#[derive(Clone, Copy)]
struct GroundPathSurfaceOverlay {
    cell: IVec3,
    texture: AssetId,
    mask: AssetId,
    mirror_x: bool,
    mirror_y: bool,
}

/// Keeps path masks and surface textures loaded while terrain images use them.
pub(super) fn retain_ground_path_surface_image_dependencies(
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut dependencies: ResMut<GroundPathSurfaceImageDependencies>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let identifiers = AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS
        .into_iter()
        .map(AssetId::from_virtual_path)
        .chain(
            definitions
                .paths()
                .filter(|definition| !definition.elevated)
                .map(|definition| AssetId(definition.surface_texture.0)),
        );
    for identifier in identifiers {
        if dependencies.images.contains_key(&identifier) {
            continue;
        }
        if let Some(image) = definitions.texture_image(identifier) {
            dependencies.images.insert(identifier, image);
        }
    }
}

/// Invalidates only terrain chunks whose composed image can contain a changed
/// path surface. Asset reloads invalidate all chunks because an existing mask
/// or path texture may have changed without topology changing.
#[allow(clippy::too_many_arguments)]
pub(super) fn mark_terrain_chunks_requiring_ground_path_surface_recomposition(
    mut commands: Commands,
    mut topology_changed: MessageReader<TopologyChanged>,
    added_paths: Query<&PathTile, Added<PathTile>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut dependencies: ResMut<GroundPathSurfaceImageDependencies>,
    grid: Res<TopologyGrid>,
    paths: Query<&PathTile>,
    chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&GroundPathSurfaceProjectionRevision>,
    )>,
) {
    let dependency_image_changed = image_events.read().fold(false, |changed, event| {
        let event_id = match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id }
            | AssetEvent::LoadedWithDependencies { id } => *id,
        };
        changed
            || dependencies
                .used_by_projected_paths
                .iter()
                .filter_map(|identifier| dependencies.images.get(identifier))
                .any(|handle| handle.id() == event_id)
    });
    let definition_changed = definition_events.read().fold(false, |changed, event| {
        let relevant_definition_changed = match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => {
                if definitions
                    .get(*id)
                    .is_some_and(WorldDefinitionAsset::contains_ground_path_surface_definitions)
                {
                    dependencies
                        .definition_assets_containing_ground_path_surfaces
                        .insert(*id);
                    true
                } else {
                    dependencies
                        .definition_assets_containing_ground_path_surfaces
                        .remove(id)
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => dependencies
                .definition_assets_containing_ground_path_surfaces
                .remove(id),
        };
        changed || relevant_definition_changed
    });
    if dependency_image_changed || definition_changed {
        let surface_patch_margin = grid.spacing_m * 5.0;
        for (entity, chunk, projected) in &chunks {
            if projected.is_none() {
                continue;
            }
            let chunk_cells = f32::from(chunk.side.saturating_sub(1));
            let chunk_min = chunk.origin;
            let chunk_max = chunk.origin + Vec2::splat(chunk_cells * chunk.spacing_m);
            if !paths.iter().any(|path| {
                let path_origin = grid.cell_translation(path.cell).xz();
                path_origin
                    .cmple(chunk_max + Vec2::splat(surface_patch_margin))
                    .all()
                    && path_origin
                        .cmpge(chunk_min - Vec2::splat(surface_patch_margin))
                        .all()
            }) {
                continue;
            }
            commands
                .entity(entity)
                .insert(GroundPathSurfaceProjectionDirty);
        }
        return;
    }

    let changed_bounds = topology_changed
        .read()
        .map(|change| change.bounds)
        .chain(
            added_paths
                .iter()
                .map(|path| IRect::from_corners(path.cell.xy(), path.cell.xy())),
        )
        .reduce(|current, next| current.union(next));
    let Some(changed_bounds) = changed_bounds else {
        return;
    };
    let surface_patch_margin = grid.spacing_m * 5.0;
    let world_min = grid.cell_translation(changed_bounds.min.extend(0)).xz()
        - Vec2::splat(surface_patch_margin);
    let world_max = grid.cell_translation(changed_bounds.max.extend(0)).xz()
        + Vec2::splat(surface_patch_margin);
    for (entity, chunk, projected) in &chunks {
        if projected.is_none() {
            continue;
        }
        let chunk_cells = f32::from(chunk.side.saturating_sub(1));
        let chunk_min = chunk.origin;
        let chunk_max = chunk.origin + Vec2::splat(chunk_cells * chunk.spacing_m);
        if chunk_min.cmple(world_max).all() && chunk_max.cmpge(world_min).all() {
            commands
                .entity(entity)
                .insert(GroundPathSurfaceProjectionDirty);
        }
    }
}

/// Rebuilds a terrain chunk's base image and applies the
/// ground-path surface-patch records directly to that owned image.
#[allow(clippy::too_many_arguments)]
pub(super) fn recompose_ground_path_surfaces_into_terrain_chunk_images(
    mut commands: Commands,
    mut reported_surface_readiness: Local<Option<(usize, usize, usize)>>,
    mut images: ResMut<Assets<Image>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut dependencies: ResMut<GroundPathSurfaceImageDependencies>,
    grid: Res<TopologyGrid>,
    paths: Query<(Entity, &PathTile)>,
    fences: Query<&FenceEdge>,
    nodes: Query<&TopologyNode>,
    chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainSurfaceImage,
        &TerrainRenderRevision,
        Option<&GroundPathSurfaceProjectionRevision>,
        Option<&GroundPathSurfaceProjectionDirty>,
    )>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if chunks
        .iter()
        .all(|(_, _, _, _, render_revision, projected, dirty)| {
            dirty.is_none() && projected.is_some_and(|projected| projected.0 == render_revision.0)
        })
    {
        return;
    }
    let source_grid_offset = (Vec2::new(grid.origin.x, -grid.origin.y) / grid.spacing_m)
        .round()
        .as_ivec2()
        .extend(0);
    let curb_edges = fences
        .iter()
        .filter_map(|edge| {
            let (Ok(a), Ok(b)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
                return None;
            };
            let a = source_grid_offset + IVec3::new(a.cell.x, -a.cell.y, a.cell.z);
            let b = source_grid_offset + IVec3::new(b.cell.x, -b.cell.y, b.cell.z);
            EdgeKey::new(a, b).map(|key| ((edge.definition, key), (a, b)))
        })
        .collect::<HashMap<_, _>>();
    let mut overlays = paths
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
            Some((
                entity,
                AssetId(definition.surface_texture.0),
                mask_texture_selections,
            ))
        })
        .flat_map(|(entity, texture, selections)| {
            selections.into_iter().flatten().map(move |selection| {
                (
                    entity,
                    GroundPathSurfaceOverlay {
                        cell: selection.cell,
                        texture,
                        mask: AssetId::from_virtual_path(selection.source_path),
                        mirror_x: selection.mirror_mode & 1 != 0,
                        mirror_y: selection.mirror_mode & 2 != 0,
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    overlays.sort_unstable_by_key(|(entity, overlay)| {
        (-overlay.cell.y, overlay.cell.x, entity.to_bits())
    });
    dependencies.used_by_projected_paths.clear();
    dependencies.used_by_projected_paths.extend(
        overlays
            .iter()
            .flat_map(|(_, overlay)| [overlay.texture, overlay.mask]),
    );
    let missing_images = dependencies
        .used_by_projected_paths
        .iter()
        .filter(|identifier| {
            dependencies
                .images
                .get(*identifier)
                .is_none_or(|handle| images.get(handle).is_none())
        })
        .count();
    let readiness = (paths.iter().count(), overlays.len(), missing_images);
    if *reported_surface_readiness != Some(readiness) {
        debug!(
            paths = readiness.0,
            overlays = readiness.1,
            missing_images,
            retained_images = dependencies.images.len(),
            "ground path surface readiness"
        );
        *reported_surface_readiness = Some(readiness);
    }
    if missing_images != 0 {
        return;
    }

    for (entity, chunk, _, surface, render_revision, projected, dirty) in &chunks {
        if dirty.is_none() && projected.is_some_and(|projected| projected.0 == render_revision.0) {
            continue;
        }
        let chunk_cells = f32::from(chunk.side.saturating_sub(1));
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
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
        if relevant_overlays.is_empty() && dirty.is_none() {
            commands
                .entity(entity)
                .insert(GroundPathSurfaceProjectionRevision(render_revision.0));
            continue;
        }
        let mut composed = if dirty.is_some() {
            let edited_chunks = chunks
                .iter()
                .filter(|(_, candidate, ..)| {
                    candidate.asset == chunk.asset
                        && (candidate.coord - chunk.coord)
                            .abs()
                            .cmple(IVec2::ONE)
                            .all()
                })
                .filter_map(|(_, candidate, edited, ..)| {
                    edited.map(|edited| (candidate.asset.clone(), candidate.coord, edited.clone()))
                })
                .collect::<Vec<_>>();
            let Some(composed) =
                compose_terrain_surface_image(chunk, asset, &edited_chunks, &images)
            else {
                continue;
            };
            composed
        } else {
            let Some(composed) = images.get(&surface.0).cloned() else {
                continue;
            };
            composed
        };
        for overlay in relevant_overlays {
            let Some((path_texture, detail_mask)) = (|| {
                let path_texture = dependencies.images.get(&overlay.texture)?;
                let detail_mask = dependencies.images.get(&overlay.mask)?;
                Some((images.get(path_texture)?, images.get(detail_mask)?))
            })() else {
                continue;
            };
            if blend_ground_path_surface_patch_into_terrain_surface_image(
                &mut composed,
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
        let Some(mut image) = images.get_mut(&surface.0) else {
            continue;
        };
        *image = composed;
        commands
            .entity(entity)
            .insert(GroundPathSurfaceProjectionRevision(render_revision.0))
            .remove::<GroundPathSurfaceProjectionDirty>();
    }
}

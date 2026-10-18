use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::{
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_edit_data_types::{TerrainEditDelta, TerrainSample, TerrainWaterAffiliation},
    terrain_sample_grid_queries::{
        authored_terrain_sample_index, read_materialized_terrain_sample,
    },
    terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionEditDelta,
        TerrainWaterRegionQuery, TerrainWaterRegionRetired,
    },
    terrain_water_view_state::{TerrainWaterRegionFade, TerrainWaterRegionVisibility},
};

/// Splits only existing affiliation; material/style similarity never creates links.
#[allow(clippy::too_many_arguments)]
pub(in crate::terrain) fn split_disconnected_water_affiliations<'a>(
    commands: &mut Commands,
    asset: &TerrainAsset,
    map_parent: Entity,
    membership: &TerrainWaterMembershipIndex,
    regions: &TerrainWaterRegionQuery,
    chunks: impl Iterator<
        Item = (
            &'a TerrainChunk,
            Option<&'a EditedTerrainSamples>,
            TerrainChunkId,
        ),
    >,
    deltas: &mut Vec<TerrainEditDelta>,
    owner_changes: &mut Vec<TerrainWaterRegionEditDelta>,
) -> Option<()> {
    let chunks: Vec<_> = chunks
        .filter(|(chunk, _, _)| chunk.asset.id() == membership.terrain())
        .collect();
    let grid = asset.canonical_terrain_grid();
    let mut changed_points = BTreeMap::new();
    let mut affected = BTreeSet::new();
    for delta in deltas.iter() {
        let (chunk, _, _) = chunks.iter().find(|(_, _, id)| *id == delta.chunk)?;
        let width = (delta.max.x - delta.min.x + 1) as usize;
        for (offset, (before, after)) in delta.before.iter().zip(&delta.after).enumerate() {
            if before.water_affiliation == after.water_affiliation {
                continue;
            }
            let x = delta.min.x as usize + offset % width;
            let z = delta.min.y as usize + offset / width;
            let index = authored_terrain_sample_index(chunk, asset, x, z)? as u32;
            let point = UVec2::new(index % grid.width, index / grid.width);
            let old = membership.point_owner(point);
            let next = match after.water_affiliation {
                Some(TerrainWaterAffiliation::Edited(owner)) => Some(owner),
                Some(TerrainWaterAffiliation::SourceRow(_)) => old,
                Some(TerrainWaterAffiliation::Unresolved) => return None,
                None => None,
            };
            changed_points.insert(index, next);
            if old != next {
                affected.extend(old);
                affected.extend(next);
            }
        }
    }
    affected.extend(
        owner_changes
            .iter()
            .filter(|change| change.after_active)
            .map(|change| change.owner),
    );
    let mut assignments = BTreeMap::new();
    for owner in affected {
        if owner_changes
            .iter()
            .rev()
            .find(|change| change.owner == owner)
            .is_some_and(|change| !change.after_active)
        {
            continue;
        }
        let mut points: BTreeSet<_> = membership
            .accepted_points(owner)
            .map(|point| point.y * grid.width + point.x)
            .collect();
        for (&point, &next) in &changed_points {
            if next == Some(owner) {
                points.insert(point);
            } else {
                points.remove(&point);
            }
        }
        let components = connected_components(points, grid.width, grid.height);
        if components.len() == 1 {
            continue;
        }
        let existing = owner_changes
            .iter()
            .rev()
            .find(|change| change.owner == owner);
        let state = if let Some(state) = existing.and_then(|change| change.after.clone()) {
            state
        } else {
            let region = regions.get(owner)?;
            let biome = region.biome(asset)?;
            TerrainWaterRegionEdit {
                terrain: region.terrain().clone(),
                biome_index: grid
                    .biomes
                    .iter()
                    .position(|entry| std::ptr::eq(entry, biome))?,
                surface_height_metres: region.surface_height(asset)?,
                depth_ratio: region.depth_ratio(asset)?,
                uses_tank_minimum_depth: region.uses_tank_minimum_depth(asset)?,
            }
        };
        if components.is_empty() {
            if let Some(change) = owner_changes
                .iter_mut()
                .rev()
                .find(|change| change.owner == owner)
            {
                change.after_active = false;
            } else {
                owner_changes.push(TerrainWaterRegionEditDelta {
                    owner,
                    before: regions.get(owner)?.edited_state().cloned(),
                    after: None,
                    before_active: true,
                    after_active: false,
                });
            }
            continue;
        }
        // The first component contains the lowest source point, matching the
        // ordered point traversal; its owner and effect phases are retained.
        for component in components.into_iter().skip(1) {
            let child = commands
                .spawn((
                    ChildOf(map_parent),
                    TerrainWaterRegionRetired,
                    TerrainWaterRegionFade::default(),
                    TerrainWaterRegionVisibility::default(),
                ))
                .id();
            owner_changes.push(TerrainWaterRegionEditDelta {
                owner: child,
                before: None,
                after: Some(state.clone()),
                before_active: false,
                after_active: true,
            });
            for point in component {
                assignments.insert(point, child);
            }
        }
    }
    for (chunk, edited, id) in chunks {
        let cells = u32::from(chunk.side.checked_sub(1)?);
        let minimum = chunk.source_coord.as_uvec2() * cells;
        let maximum = minimum + UVec2::splat(cells);
        let local_changes: Vec<_> = assignments
            .iter()
            .filter_map(|(&point, &owner)| {
                let point = UVec2::new(point % grid.width, point / grid.width);
                (point.cmpge(minimum).all() && point.cmple(maximum).all()).then(|| {
                    (
                        UVec2::new(point.x - minimum.x, cells - (point.y - minimum.y)),
                        owner,
                    )
                })
            })
            .collect();
        if local_changes.is_empty() {
            continue;
        }
        let previous = deltas
            .iter()
            .position(|delta| delta.chunk == id)
            .map(|index| deltas.remove(index));
        let mut min = local_changes
            .iter()
            .map(|(point, _)| *point)
            .reduce(UVec2::min)?;
        let mut max = local_changes
            .iter()
            .map(|(point, _)| *point)
            .reduce(UVec2::max)?;
        if let Some(previous) = &previous {
            min = min.min(previous.min);
            max = max.max(previous.max);
        }
        let width = (max.x - min.x + 1) as usize;
        let mut before = Vec::with_capacity(width * (max.y - min.y + 1) as usize);
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                before.push(read_materialized_terrain_sample(
                    chunk, asset, edited, x as usize, z as usize,
                )?);
            }
        }
        let mut after = before.clone();
        if let Some(previous) = previous {
            overlay_delta_image(
                &mut before,
                min,
                width,
                previous.min,
                previous.max,
                &previous.before,
            );
            overlay_delta_image(
                &mut after,
                min,
                width,
                previous.min,
                previous.max,
                &previous.after,
            );
        }
        for (point, owner) in local_changes {
            after[(point.y - min.y) as usize * width + (point.x - min.x) as usize]
                .water_affiliation = Some(TerrainWaterAffiliation::Edited(owner));
        }
        deltas.push(TerrainEditDelta {
            chunk: id,
            min,
            max,
            before: before.into_boxed_slice(),
            after: after.into_boxed_slice(),
        });
    }
    Some(())
}

fn overlay_delta_image(
    destination: &mut [TerrainSample],
    min: UVec2,
    width: usize,
    source_min: UVec2,
    source_max: UVec2,
    source: &[TerrainSample],
) {
    let source_width = (source_max.x - source_min.x + 1) as usize;
    for (index, sample) in source.iter().enumerate() {
        let x = source_min.x as usize + index % source_width - min.x as usize;
        let z = source_min.y as usize + index / source_width - min.y as usize;
        destination[z * width + x] = *sample;
    }
}

fn connected_components(mut points: BTreeSet<u32>, width: u32, height: u32) -> Vec<Vec<u32>> {
    let mut components = Vec::new();
    while let Some(seed) = points.pop_first() {
        let mut component = vec![seed];
        let mut current = 0;
        while current < component.len() {
            let point = component[current];
            let x = point % width;
            let y = point / width;
            for next in [
                (x > 0).then(|| point - 1),
                (x + 1 < width).then(|| point + 1),
                (y > 0).then(|| point - width),
                (y + 1 < height).then(|| point + width),
            ]
            .into_iter()
            .flatten()
            {
                if points.remove(&next) {
                    component.push(next);
                }
            }
            current += 1;
        }
        components.push(component);
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cardinal_links_keep_source_order_without_diagonal_or_row_wrap_links() {
        assert_eq!(
            connected_components([0, 1, 5, 8, 12, 15].into_iter().collect(), 4, 4),
            vec![vec![0, 1, 5], vec![8, 12], vec![15]]
        );
        assert_eq!(
            connected_components([3, 4].into_iter().collect(), 4, 4),
            vec![vec![3], vec![4]]
        );
    }
}

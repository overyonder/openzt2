use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery, TerrainWaterRegionSource,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

mod height_support;
mod selector_projection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::terrain) enum WaterMembershipProjectionStatus {
    HeightSupportUnresolved,
    DefinitionsPending,
    TerrainPending,
    SourceOwnerMissing(usize),
    SourceAcceptanceUnresolved,
    EditedAffiliationUnresolved,
    SelectorProjectionUnresolved,
    Ready,
}

fn report_membership_status(
    previous: &mut Option<(
        bevy::asset::AssetId<TerrainAsset>,
        WaterMembershipProjectionStatus,
    )>,
    terrain: bevy::asset::AssetId<TerrainAsset>,
    status: WaterMembershipProjectionStatus,
) {
    if *previous != Some((terrain, status)) {
        info!(target: "openzt2_water_membership", ?terrain, ?status, "water membership projection status changed");
        *previous = Some((terrain, status));
    }
}

/// An absent index denotes unresolved ownership, not a dry terrain.
pub(in crate::terrain) fn project_terrain_water_membership(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    supports: height_support::TerrainHeightSupports,
    chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        Option<&TerrainWaterMembershipIndex>,
    )>,
    regions: Query<(Entity, &TerrainWaterRegionSource)>,
    live_regions: TerrainWaterRegionQuery,
    mut changes: MessageReader<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>,
    active_definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    selector_fences: selector_projection::SelectorFences,
    topology: Res<TopologyGrid>,
    mut diagnostic: Local<
        Option<(
            bevy::asset::AssetId<TerrainAsset>,
            WaterMembershipProjectionStatus,
        )>,
    >,
) {
    let changed_chunks: Vec<_> = changes.read().map(|change| change.chunk).collect();
    let Some((_, root, _, _)) = chunks
        .iter()
        .find(|(_, chunk, _, _)| chunk.source_coord == IVec2::ZERO)
    else {
        *diagnostic = None;
        return;
    };
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        report_membership_status(
            &mut diagnostic,
            root.asset.id(),
            WaterMembershipProjectionStatus::DefinitionsPending,
        );
        return;
    };
    let curb = openzt2_game_data::AssetId::from_key("curb");
    let empty_selector_topology = !selector_fences.iter().any(|(_, edge, _, _)| {
        definitions
            .find_fence(edge.definition)
            .and_then(|fence| definitions.find_object(fence.object))
            .or_else(|| definitions.find_object(edge.definition))
            .is_none_or(|object| !object.behavior_type_identifiers.contains(&curb))
    });
    for (entity, chunk, _, index) in &chunks {
        if chunk.source_coord != IVec2::ZERO {
            continue;
        }
        let has_edits = chunks.iter().any(|(_, candidate, edited, _)| {
            candidate.asset.id() == chunk.asset.id() && edited.is_some()
        });
        let Some(asset) = assets.get(&chunk.asset) else {
            continue;
        };
        let Some(height_limits) = supports.point_limits(asset, definitions, &topology) else {
            commands
                .entity(entity)
                .remove::<TerrainWaterMembershipIndex>();
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::HeightSupportUnresolved,
            );
            continue;
        };
        let Some((selector_projection, fence_probes)) =
            selector_projection::construct_selector_projection(
                asset,
                definitions,
                &topology,
                &selector_fences,
            )
        else {
            if index.is_some() {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SelectorProjectionUnresolved,
            );
            continue;
        };
        let region_revision = live_regions.source_revision(chunk.asset.id());
        if let Some(index) = index {
            if chunks.iter().any(|(entity, candidate, _, _)| {
                candidate.asset == chunk.asset && changed_chunks.contains(&entity)
            }) || index.empty_selector_topology() != empty_selector_topology
                || !index.selector_projection_matches(&selector_projection)
                || !index.fence_probes_match(&fence_probes)
                || !index.height_support_limits_match(&height_limits)
                || index.region_source_revision() != region_revision
                || assets.is_changed()
                || active_definitions.is_changed()
                || definition_assets.is_changed()
            {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            } else {
                report_membership_status(
                    &mut diagnostic,
                    chunk.asset.id(),
                    WaterMembershipProjectionStatus::Ready,
                );
                continue;
            }
        }
        let Some(asset) = assets.get(&chunk.asset) else {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::TerrainPending,
            );
            continue;
        };
        let grid = asset.canonical_terrain_grid();
        let mut owners = vec![None; grid.water_regions.len()];
        for (owner, source) in &regions {
            if source.terrain().id() == chunk.asset.id() {
                if let Some(entry) = owners.get_mut(source.source_region_index()) {
                    *entry = Some(owner);
                }
            }
        }
        if let Some(row) = owners
            .iter()
            .zip(&grid.water_regions)
            .position(|(owner, row)| owner.is_none() && !row.row_major_cell_indices.is_empty())
        {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SourceOwnerMissing(row),
            );
            continue;
        }
        let Some(mut index) = settle_source_membership(
            chunk.asset.id(),
            asset,
            &owners,
            empty_selector_topology,
            height_limits,
            &live_regions,
            definitions
                .tank_depth_policy()
                .map(|policy| policy.minimum_water_depth_cm as f32 * 0.01),
        ) else {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SourceAcceptanceUnresolved,
            );
            continue;
        };
        if has_edits {
            let Some(edited_index) = project_edited_membership(
                asset,
                index,
                chunks.iter().filter_map(|(_, candidate, edited, _)| {
                    (candidate.asset == chunk.asset).then_some((candidate, edited))
                }),
                &live_regions,
            ) else {
                report_membership_status(
                    &mut diagnostic,
                    chunk.asset.id(),
                    WaterMembershipProjectionStatus::EditedAffiliationUnresolved,
                );
                continue;
            };
            index = edited_index;
        }
        let Some(index) = index
            .with_selector_projection(selector_projection)
            .and_then(|index| index.with_fence_probes(fence_probes))
        else {
            continue;
        };
        commands
            .entity(entity)
            .insert(index.with_region_source_revision(region_revision));
        report_membership_status(
            &mut diagnostic,
            chunk.asset.id(),
            WaterMembershipProjectionStatus::Ready,
        );
    }
}

fn settle_source_membership(
    asset_id: bevy::asset::AssetId<TerrainAsset>,
    asset: &TerrainAsset,
    owners: &[Option<Entity>],
    empty_selector_topology: bool,
    height_limits: Box<[[f32; 2]]>,
    live_regions: &TerrainWaterRegionQuery,
    tank_minimum_depth: Option<f32>,
) -> Option<TerrainWaterMembershipIndex> {
    let grid = asset.canonical_terrain_grid();
    let tile_width = grid.width.checked_sub(1)? / 4;
    let tile_height = grid.height.checked_sub(1)? / 4;
    if tile_width == 0
        || tile_height == 0
        || (grid.width - 1) % 4 != 0
        || (grid.height - 1) % 4 != 0
    {
        return None;
    }
    let mut points = vec![None; grid.samples.len()];
    let mut sides = vec![[None; 4]; (tile_width * tile_height) as usize];
    for (row_index, region) in grid.water_regions.iter().enumerate() {
        let Some(owner) = owners[row_index] else {
            continue;
        };
        let Some(view) = live_regions.get(owner) else {
            continue;
        };
        let biome = view.biome(asset)?;
        let plane = view.surface_height(asset)?;
        let high = if view.uses_tank_minimum_depth(asset)? {
            plane - tank_minimum_depth?
        } else {
            plane + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
        };
        let low = plane - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
        let mut refreshed_tiles = Vec::new();
        for &index in &region.row_major_cell_indices {
            let point = UVec2::new(index % grid.width, index / grid.width);
            if index as usize >= points.len() {
                return None;
            }
            asset.sample_authored_water_region_index(index as usize)?;
            let neighbours = neighbouring_tiles(point, tile_width, tile_height);
            let mut accepted = true;
            'tiles: for tile in neighbours.iter().copied() {
                let side_slots = &mut sides[(tile.y * tile_width + tile.x) as usize];
                // Side validation commits as it walks; a later failure does
                // not roll back side assignments made earlier in this call.
                for side in [0, 2, 1, 3] {
                    let bucket = TerrainWaterMembershipIndex::side_bucket(tile, side)?;
                    if !bucket.contains(&point) {
                        continue;
                    }
                    if side_slots[side].is_some_and(|current| current != owner) {
                        accepted = false;
                        break 'tiles;
                    }
                    if side_slots[side].is_none()
                        && (!side_accepts_path_exclusions(tile, side, |point| {
                            grid.samples[(point.y * grid.width + point.x) as usize]
                                .excludes_elevated_path
                        }) || !bucket.iter().all(|point| {
                            TerrainWaterMembershipIndex::height_limits_support_water_interval(
                                height_limits[(point.y * grid.width + point.x) as usize],
                                *point,
                                low,
                                high,
                                asset,
                            )
                        }))
                    {
                        accepted = false;
                        break 'tiles;
                    }
                    side_slots[side] = Some(owner);
                }
            }
            points[index as usize] = accepted.then_some(owner);
            if accepted {
                refreshed_tiles.extend(neighbours);
            }
        }
        refreshed_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
        refreshed_tiles.dedup();
        for tile in refreshed_tiles {
            let slots = &mut sides[(tile.y * tile_width + tile.x) as usize];
            for (side, slot) in slots.iter_mut().enumerate() {
                *slot = TerrainWaterMembershipIndex::side_bucket(tile, side)?
                    .into_iter()
                    .filter_map(|point| points[(point.y * grid.width + point.x) as usize])
                    .last();
            }
            if slots[0] == slots[2] {
                if slots[1].is_none() {
                    slots[1] = slots[0];
                }
                if slots[3].is_none() {
                    slots[3] = slots[0];
                }
            }
            if slots[1] == slots[3] {
                if slots[0].is_none() {
                    slots[0] = slots[1];
                }
                if slots[2].is_none() {
                    slots[2] = slots[1];
                }
            }
            let flagged = |point: UVec2| {
                grid.samples[((tile.y * 4 + point.y) * grid.width + tile.x * 4 + point.x) as usize]
                    .water_surface_flag
            };
            let plane_for_owner = |owner: Entity| live_regions.get(owner)?.surface_height(asset);
            if flagged(UVec2::new(1, 3)) || flagged(UVec2::new(3, 1)) {
                merge_flagged_sides(slots, 2, 3, plane_for_owner);
                merge_flagged_sides(slots, 0, 1, plane_for_owner);
            }
            if flagged(UVec2::new(3, 3)) || flagged(UVec2::new(1, 1)) {
                merge_flagged_sides(slots, 2, 1, plane_for_owner);
                merge_flagged_sides(slots, 0, 3, plane_for_owner);
            }
            for side in 0..4 {
                if slots[side].is_some()
                    && slots[(side + 1) % 4].is_none()
                    && slots[(side + 3) % 4].is_none()
                {
                    slots[(side + 1) % 4] = slots[side];
                }
            }
        }
    }
    TerrainWaterMembershipIndex::new(
        asset_id,
        grid.width,
        grid.height,
        points.into_boxed_slice(),
        sides.into_boxed_slice(),
        empty_selector_topology,
    )?
    .with_height_support_limits(height_limits)
    .map(|index| index.with_tank_minimum_depth(tank_minimum_depth))
}

fn project_edited_membership<'a>(
    asset: &TerrainAsset,
    base: TerrainWaterMembershipIndex,
    chunks: impl Iterator<Item = (&'a TerrainChunk, Option<&'a EditedTerrainSamples>)>,
    regions: &TerrainWaterRegionQuery,
) -> Option<TerrainWaterMembershipIndex> {
    use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
    use openzt2_zoo_state::terrain::terrain_sample_grid_queries::{
        authored_terrain_sample_index, read_materialized_terrain_sample,
    };
    let grid = asset.canonical_terrain_grid();
    let tile_width = (grid.width - 1) / 4;
    let tile_height = (grid.height - 1) / 4;
    let mut points: Vec<_> = (0..grid.samples.len())
        .map(|index| {
            base.point_owner(UVec2::new(
                index as u32 % grid.width,
                index as u32 / grid.width,
            ))
        })
        .collect();
    let mut heights: Vec<_> = grid
        .samples
        .iter()
        .map(|point| grid.units.base_height_metres + point.height_relative_to_base_metres)
        .collect();
    let mut sides: Vec<_> = (0..tile_width * tile_height)
        .map(|index| {
            *base
                .side_owners(UVec2::new(index % tile_width, index / tile_width))
                .expect("source tile dimensions are validated")
        })
        .collect();
    let mut dirty_tiles = Vec::new();
    let mut edited_affiliation = vec![false; points.len()];
    for (chunk, edited) in chunks {
        let Some(edited) = edited else {
            continue;
        };
        for z in 0..usize::from(chunk.side) {
            for x in 0..usize::from(chunk.side) {
                let index = authored_terrain_sample_index(chunk, asset, x, z)?;
                let sample = read_materialized_terrain_sample(chunk, asset, Some(edited), x, z)?;
                let point = UVec2::new(index as u32 % grid.width, index as u32 / grid.width);
                let owner = match sample.water_affiliation {
                    Some(TerrainWaterAffiliation::SourceRow(_)) => base.point_owner(point),
                    Some(TerrainWaterAffiliation::Edited(owner)) => {
                        edited_affiliation[index] = true;
                        if let Some(region) = regions.get(owner) {
                            if region.terrain().id() != chunk.asset.id() {
                                return None;
                            }
                            Some(owner)
                        } else {
                            None
                        }
                    }
                    Some(TerrainWaterAffiliation::Unresolved) => return None,
                    None => None,
                };
                let height = f32::from(sample.height_cm) * 0.01;
                if points[index] != owner || heights[index] != height {
                    dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
                }
                points[index] = owner;
                heights[index] = height;
            }
        }
    }
    dirty_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
    dirty_tiles.dedup();
    for (index, owner) in points.iter_mut().enumerate() {
        let Some(current_owner) = *owner else {
            continue;
        };
        let point = UVec2::new(index as u32 % grid.width, index as u32 / grid.width);
        let Some(region) = regions.get(current_owner) else {
            *owner = None;
            dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
            continue;
        };
        if !edited_affiliation[index] {
            continue;
        }
        let biome = region.biome(asset)?;
        let plane = region.surface_height(asset)?;
        let low = plane - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
        let high = if region.uses_tank_minimum_depth(asset)? {
            plane - base.tank_minimum_depth()?
        } else {
            plane + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
        };
        if !neighbouring_tiles(point, tile_width, tile_height)
            .iter()
            .all(|tile| {
                (0..4).all(|side| {
                    let Some(bucket) = TerrainWaterMembershipIndex::side_bucket(*tile, side) else {
                        return false;
                    };
                    !bucket.contains(&point)
                        || (side_accepts_path_exclusions(*tile, side, |point| {
                            grid.samples[(point.y * grid.width + point.x) as usize]
                                .excludes_elevated_path
                        }) && bucket
                            .iter()
                            .all(|point| base.supports_water_interval(*point, low, high, asset)))
                })
            })
        {
            *owner = None;
            dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
        }
    }
    dirty_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
    dirty_tiles.dedup();
    for tile in dirty_tiles {
        let slots = &mut sides[(tile.y * tile_width + tile.x) as usize];
        for (side, slot) in slots.iter_mut().enumerate() {
            *slot = TerrainWaterMembershipIndex::side_bucket(tile, side)?
                .into_iter()
                .filter_map(|point| points[(point.y * grid.width + point.x) as usize])
                .last();
        }
        for (first, opposite) in [(0, 2), (1, 3)] {
            if slots[first] == slots[opposite] {
                for adjacent in [(first + 1) % 4, (first + 3) % 4] {
                    if slots[adjacent].is_none() {
                        slots[adjacent] = slots[first];
                    }
                }
            }
        }
        let flagged = |local: UVec2| {
            grid.samples[((tile.y * 4 + local.y) * grid.width + tile.x * 4 + local.x) as usize]
                .water_surface_flag
        };
        let plane = |owner| regions.get(owner)?.surface_height(asset);
        if flagged(UVec2::new(1, 3)) || flagged(UVec2::new(3, 1)) {
            merge_flagged_sides(slots, 2, 3, plane);
            merge_flagged_sides(slots, 0, 1, plane);
        }
        if flagged(UVec2::new(3, 3)) || flagged(UVec2::new(1, 1)) {
            merge_flagged_sides(slots, 2, 1, plane);
            merge_flagged_sides(slots, 0, 3, plane);
        }
        for side in 0..4 {
            if slots[side].is_some()
                && slots[(side + 1) % 4].is_none()
                && slots[(side + 3) % 4].is_none()
            {
                slots[(side + 1) % 4] = slots[side];
            }
        }
    }
    TerrainWaterMembershipIndex::new(
        base.terrain(),
        grid.width,
        grid.height,
        points.into_boxed_slice(),
        sides.into_boxed_slice(),
        base.empty_selector_topology(),
    )?
    .with_selector_projection_from(&base)?
    .with_edited_point_heights(heights.into_boxed_slice())
}

fn neighbouring_tiles(point: UVec2, width: u32, height: u32) -> Vec<UVec2> {
    let tile = point / 4;
    let mut neighbours = vec![tile];
    if point.x % 4 == 0 && tile.x > 0 {
        neighbours.push(tile - UVec2::X);
    }
    if point.y % 4 == 0 && tile.y > 0 {
        neighbours.push(tile - UVec2::Y);
    }
    if point.x % 4 == 0 && point.y % 4 == 0 && tile.x > 0 && tile.y > 0 {
        neighbours.push(tile - UVec2::ONE);
    }
    neighbours.retain(|tile| tile.x < width && tile.y < height);
    neighbours
}

#[cfg(test)]
fn bucket_height_supported(
    bucket: &[UVec2; 9],
    flagged: impl Fn(UVec2) -> bool,
    supported_height: bool,
) -> bool {
    bucket
        .iter()
        .all(|point| flagged(*point) || supported_height)
}

fn side_accepts_path_exclusions(
    tile: UVec2,
    side: usize,
    excluded: impl Fn(UVec2) -> bool,
) -> bool {
    let origin = tile * 4;
    let [a, b, c, d, centre] = [
        UVec2::ZERO,
        UVec2::new(4, 0),
        UVec2::splat(4),
        UVec2::new(0, 4),
        UVec2::splat(2),
    ]
    .map(|point| excluded(origin + point));
    match side {
        0 => !(c || d || (!centre && a && b)),
        1 => !(b || d || (!centre && a && c)),
        2 => !(a || b || (!centre && c && d)),
        3 => !(a || c || (!centre && b && d)),
        _ => false,
    }
}

fn merge_flagged_sides(
    slots: &mut [Option<Entity>; 4],
    first: usize,
    second: usize,
    height: impl Fn(Entity) -> Option<f32>,
) {
    let winner = match (slots[first], slots[second]) {
        (Some(left), Some(right)) => {
            if height(left) > height(right) {
                Some(left)
            } else {
                Some(right)
            }
        }
        (Some(owner), None) | (None, Some(owner)) => Some(owner),
        (None, None) => None,
    };
    slots[first] = winner;
    slots[second] = winner;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_height_relations_do_not_require_a_water_support_adapter() {
        let mut point = openzt2_game_data::terrain::TerrainSample {
            height_relative_to_base_metres: 13.0,
            biome_index: 0,
            has_ground_cover: false,
            water_depth: openzt2_game_data::terrain::TerrainWaterDepth::Dry,
            water_surface_flag: false,
            excludes_elevated_path: false,
            support_floor_metres: Some(-1000.0),
            height_cap_metres: Some(1000.0),
            height_is_linked: false,
            has_child_height_link: true,
        };
        assert!(side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
        point.height_is_linked = true;
        assert!(side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
        point.excludes_elevated_path = true;
        assert!(!side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
    }

    #[test]
    fn final_point_row_and_column_keep_the_interior_tile() {
        assert_eq!(neighbouring_tiles(UVec2::new(8, 8), 2, 2), vec![UVec2::ONE]);
        assert_eq!(neighbouring_tiles(UVec2::new(8, 2), 2, 2), vec![UVec2::X]);
        assert_eq!(neighbouring_tiles(UVec2::new(2, 8), 2, 2), vec![UVec2::Y]);
    }

    #[test]
    fn flagged_points_skip_height_support_but_other_bucket_points_do_not() {
        let bucket = TerrainWaterMembershipIndex::side_bucket(UVec2::ZERO, 0).expect("valid side");
        assert!(bucket_height_supported(&bucket, |_| true, false));
        assert!(!bucket_height_supported(
            &bucket,
            |point| point != UVec2::new(2, 2),
            false
        ));
        assert!(bucket_height_supported(&bucket, |_| false, true));
    }
}

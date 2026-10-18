use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::{
    terrain_brush_types::{TerrainBrushKind, TerrainBrushPreview},
    terrain_edit_data_types::{TerrainSample, TerrainWaterAffiliation},
    terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionEditDelta,
        TerrainWaterRegionQuery, TerrainWaterRegionRetired,
    },
    terrain_water_view_state::{TerrainWaterRegionFade, TerrainWaterRegionVisibility},
};

pub(in crate::terrain) mod splitting;

pub(in crate::terrain) struct WaterBrushRegionPlan {
    pub(in crate::terrain) owner: Entity,
    pub(in crate::terrain) changes: Vec<TerrainWaterRegionEditDelta>,
    merge_owners: bool,
}

impl WaterBrushRegionPlan {
    pub(in crate::terrain) fn affects_chunk(
        &self,
        chunk: &openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk,
        membership: &TerrainWaterMembershipIndex,
    ) -> bool {
        let cells = u32::from(chunk.side.saturating_sub(1));
        let minimum = chunk.source_coord.as_uvec2() * cells;
        let maximum = minimum + UVec2::splat(cells);
        self.changes.iter().any(|change| {
            membership
                .accepted_points(change.owner)
                .any(|point| point.cmpge(minimum).all() && point.cmple(maximum).all())
        })
    }
    pub(in crate::terrain) fn accepts_point(
        &self,
        point: UVec2,
        membership: &TerrainWaterMembershipIndex,
        asset: &TerrainAsset,
    ) -> bool {
        let Some(state) = self
            .changes
            .iter()
            .find(|change| {
                change.owner
                    == if self.merge_owners {
                        self.owner
                    } else {
                        membership.point_owner(point).unwrap_or(self.owner)
                    }
            })
            .and_then(|change| change.after.as_ref())
        else {
            return false;
        };
        let grid = asset.canonical_terrain_grid();
        let Some(biome) = grid.biomes.get(state.biome_index) else {
            return false;
        };
        let high = if state.uses_tank_minimum_depth {
            let Some(depth) = membership.tank_minimum_depth() else {
                return false;
            };
            state.surface_height_metres - depth
        } else {
            state.surface_height_metres + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
        };
        let low =
            state.surface_height_metres - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
        let tile = point / 4;
        for x in [
            Some(tile.x),
            (point.x % 4 == 0).then(|| tile.x.checked_sub(1)).flatten(),
        ]
        .into_iter()
        .flatten()
        {
            for y in [
                Some(tile.y),
                (point.y % 4 == 0).then(|| tile.y.checked_sub(1)).flatten(),
            ]
            .into_iter()
            .flatten()
            {
                let tile = UVec2::new(x, y);
                let Some(slots) = membership.side_owners(tile) else {
                    continue;
                };
                for (side, owner) in slots.iter().enumerate() {
                    let Some(bucket) = TerrainWaterMembershipIndex::side_bucket(tile, side) else {
                        return false;
                    };
                    if !bucket.contains(&point) {
                        continue;
                    }
                    if owner.is_some_and(|owner| {
                        !self.changes.iter().any(|change| change.owner == owner)
                    }) {
                        return false;
                    }
                    if !bucket
                        .iter()
                        .all(|point| membership.supports_water_interval(*point, low, high, asset))
                    {
                        return false;
                    }
                }
            }
        }
        true
    }
    pub(in crate::terrain) fn update_sample(
        &self,
        sample: &mut TerrainSample,
        previous_owner: Option<Entity>,
    ) {
        let owner = match sample.water_affiliation {
            Some(TerrainWaterAffiliation::Edited(owner)) if owner == self.owner => Some(owner),
            _ => previous_owner,
        };
        if !self
            .changes
            .iter()
            .any(|change| Some(change.owner) == owner)
        {
            return;
        }
        let target = if self.merge_owners {
            self.owner
        } else {
            owner.unwrap_or(self.owner)
        };
        let Some(state) = self
            .changes
            .iter()
            .find(|change| change.owner == target)
            .and_then(|change| change.after.as_ref())
        else {
            return;
        };
        sample.water_affiliation = Some(TerrainWaterAffiliation::Edited(target));
        sample.water_cm = (state.surface_height_metres * 100.0).round() as i16;
        sample.water_biome_channel = state.biome_index as u8;
        if !self.merge_owners && sample.water_cm <= sample.height_cm {
            sample.water_style = 0;
            sample.water_affiliation = None;
        }
    }

    pub(in crate::terrain) fn ground_reference_height_cm(
        &self,
        asset: &TerrainAsset,
    ) -> Option<i16> {
        let state = self
            .changes
            .iter()
            .find(|change| change.owner == self.owner)?
            .after
            .as_ref()?;
        let shore = asset
            .canonical_terrain_grid()
            .biomes
            .get(state.biome_index)?
            .water_shore_offset_mm
            .unwrap_or(0) as f32
            * 0.001;
        Some(((state.surface_height_metres + shore) * 100.0).round() as i16)
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::terrain) fn plan_water_brush_region(
    commands: &mut Commands,
    preview: &TerrainBrushPreview,
    terrain: &Handle<TerrainAsset>,
    asset: &TerrainAsset,
    map_parent: Entity,
    samples: impl Iterator<Item = (Vec2, TerrainSample)>,
    regions: &TerrainWaterRegionQuery,
    membership: &TerrainWaterMembershipIndex,
    initial_height_cm: i16,
    elapsed_seconds: f32,
) -> Option<WaterBrushRegionPlan> {
    if membership.terrain() != terrain.id() {
        return None;
    }
    if matches!(
        preview.kind,
        TerrainBrushKind::AddWater | TerrainBrushKind::RemoveWater
    ) {
        return plan_water_level_change(
            commands,
            preview,
            terrain,
            asset,
            map_parent,
            samples,
            regions,
            membership,
            elapsed_seconds,
        );
    }
    let TerrainBrushKind::PaintWater { biome, .. } = preview.kind else {
        return None;
    };
    let grid = asset.canonical_terrain_grid();
    let biome_index = grid
        .biomes
        .iter()
        .position(|entry| openzt2_game_data::AssetId::from_key(&entry.name) == biome)?;
    let shore = grid.biomes[biome_index].water_shore_offset_mm.unwrap_or(0) as f32 * 0.001;
    let mut candidates = Vec::new();
    for (world, sample) in samples {
        if world.distance_squared(preview.center) > preview.radius_m * preview.radius_m {
            continue;
        }
        let point = (Vec2::new(world.x, -world.y) / grid.units.cell_size_metres).round();
        if point.min_element() < 0.0 {
            continue;
        }
        let Some(owner) = membership.point_owner(point.as_uvec2()) else {
            continue;
        };
        if f32::from(sample.height_cm) * 0.01 < regions.get(owner)?.surface_height(asset)?
            && !candidates.contains(&owner)
        {
            candidates.push(owner);
        }
    }
    candidates.sort_unstable();
    let config = grid.presentation.water_editing;
    let compatible = candidates.iter().all(|left| {
        candidates.iter().all(|right| {
            let Some(left) = regions.get(*left) else {
                return false;
            };
            let Some(right) = regions.get(*right) else {
                return false;
            };
            left.biome(asset) == right.biome(asset)
                && left.uses_tank_minimum_depth(asset) == right.uses_tank_minimum_depth(asset)
                && left
                    .surface_height(asset)
                    .zip(right.surface_height(asset))
                    .is_some_and(|(left, right)| {
                        (left - right).abs() <= config.plane_merge_tolerance_metres
                    })
        })
    });
    if candidates.len() > 1 && !compatible {
        let centre_owner = membership.owner_at_world_xz(preview.center, asset)??;
        candidates.clear();
        candidates.push(centre_owner);
    }
    let new = candidates.is_empty();
    let owner = if new {
        commands
            .spawn((
                ChildOf(map_parent),
                TerrainWaterRegionRetired,
                TerrainWaterRegionFade::default(),
                TerrainWaterRegionVisibility::default(),
            ))
            .id()
    } else {
        *candidates.iter().max_by_key(|owner| {
            (
                membership.accepted_points(**owner).count(),
                std::cmp::Reverse(**owner),
            )
        })?
    };
    let (height, depth_ratio) = if new {
        (
            snap_water_height(f32::from(initial_height_cm) * 0.01, config) - shore,
            0.0,
        )
    } else if candidates.len() == 1 {
        let previous = regions.get(owner)?;
        let previous_shore =
            previous.biome(asset)?.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001;
        (
            previous.surface_height(asset)? + previous_shore - shore,
            previous.depth_ratio(asset)?,
        )
    } else {
        let mut height = 0.0;
        let mut depth_ratio = 0.0;
        for candidate in &candidates {
            let region = regions.get(*candidate)?;
            height += region.surface_height(asset)?;
            depth_ratio += region.depth_ratio(asset)?;
        }
        let count = candidates.len() as f32;
        (
            snap_water_height(height / count + shore, config) - shore,
            depth_ratio / count,
        )
    };
    let state = TerrainWaterRegionEdit {
        uses_tank_minimum_depth: if new {
            false
        } else {
            regions.get(owner)?.uses_tank_minimum_depth(asset)?
        },
        terrain: terrain.clone(),
        biome_index,
        surface_height_metres: height,
        depth_ratio,
    };
    let mut changes = vec![TerrainWaterRegionEditDelta {
        owner,
        before: regions
            .get(owner)
            .and_then(|view| view.edited_state().cloned()),
        after: Some(state),
        before_active: !new,
        after_active: true,
    }];
    for donor in candidates
        .into_iter()
        .filter(|candidate| *candidate != owner)
    {
        changes.push(TerrainWaterRegionEditDelta {
            owner: donor,
            before: regions.get(donor)?.edited_state().cloned(),
            after: None,
            before_active: true,
            after_active: false,
        });
    }
    Some(WaterBrushRegionPlan {
        owner,
        changes,
        merge_owners: true,
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_water_level_change(
    commands: &mut Commands,
    preview: &TerrainBrushPreview,
    terrain: &Handle<TerrainAsset>,
    asset: &TerrainAsset,
    map_parent: Entity,
    samples: impl Iterator<Item = (Vec2, TerrainSample)>,
    regions: &TerrainWaterRegionQuery,
    membership: &TerrainWaterMembershipIndex,
    elapsed_seconds: f32,
) -> Option<WaterBrushRegionPlan> {
    let mut owners = Vec::new();
    let mut dry = None;
    for (world, sample) in samples {
        if world.distance_squared(preview.center) > preview.radius_m * preview.radius_m {
            continue;
        }
        let point = (Vec2::new(world.x, -world.y)
            / asset.canonical_terrain_grid().units.cell_size_metres)
            .round();
        if let Some(owner) = point
            .cmpge(Vec2::ZERO)
            .all()
            .then(|| membership.point_owner(point.as_uvec2()))
            .flatten()
        {
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        } else if dry.is_none_or(|(distance, _)| world.distance_squared(preview.center) < distance)
        {
            dry = Some((world.distance_squared(preview.center), sample));
        }
    }
    owners.sort_unstable();
    let delta = (preview.strength_per_s.abs() * elapsed_seconds * 100.0).round()
        * 0.01
        * if matches!(preview.kind, TerrainBrushKind::RemoveWater) {
            -1.0
        } else {
            1.0
        };
    if !delta.is_finite() || delta == 0.0 {
        return None;
    }
    let mut changes = Vec::new();
    for owner in owners {
        let view = regions.get(owner)?;
        let biome = view.biome(asset)?;
        let biome_index = asset
            .canonical_terrain_grid()
            .biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))?;
        let next_height = view.surface_height(asset)? + delta;
        let low = next_height - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
        let uses_tank_minimum_depth = view.uses_tank_minimum_depth(asset)?;
        let high = if uses_tank_minimum_depth {
            next_height - membership.tank_minimum_depth()?
        } else {
            next_height + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
        };
        if !membership
            .accepted_points(owner)
            .all(|point| membership.supports_water_interval(point, low, high, asset))
        {
            continue;
        }
        changes.push(TerrainWaterRegionEditDelta {
            owner,
            before: view.edited_state().cloned(),
            before_active: true,
            after_active: true,
            after: Some(TerrainWaterRegionEdit {
                uses_tank_minimum_depth,
                terrain: terrain.clone(),
                biome_index,
                surface_height_metres: next_height,
                depth_ratio: view.depth_ratio(asset)?,
            }),
        });
    }
    if changes.is_empty() {
        if delta < 0.0 {
            return None;
        }
        let sample = dry?.1;
        let (_, channel) =
            super::find_default_water_style_for_terrain_sample(asset, None, &sample)?;
        let owner = commands
            .spawn((
                ChildOf(map_parent),
                TerrainWaterRegionRetired,
                TerrainWaterRegionFade::default(),
                TerrainWaterRegionVisibility::default(),
            ))
            .id();
        changes.push(TerrainWaterRegionEditDelta {
            owner,
            before: None,
            before_active: false,
            after_active: true,
            after: Some(TerrainWaterRegionEdit {
                uses_tank_minimum_depth: false,
                terrain: terrain.clone(),
                biome_index: usize::from(channel),
                surface_height_metres: f32::from(sample.height_cm) * 0.01 + delta,
                depth_ratio: 0.0,
            }),
        });
    }
    let owner = changes.first()?.owner;
    // A resolved index supplies the initial spatial affiliation; all affected
    // owner points are included by affects_chunk, including shared borders.
    if membership.terrain() != terrain.id() {
        return None;
    }
    Some(WaterBrushRegionPlan {
        owner,
        changes,
        merge_owners: false,
    })
}

fn snap_water_height(
    height: f32,
    config: openzt2_game_data::terrain::TerrainWaterEditingConfiguration,
) -> f32 {
    if config.height_snap_interval_metres == 0.0 {
        return height;
    }
    let scaled = (height - config.height_snap_base_metres) / config.height_snap_interval_metres;
    let fraction = scaled % 1.0;
    let snapped = if fraction < -0.5 || (fraction > 0.0 && fraction <= 0.5) {
        scaled.floor()
    } else if fraction > 0.5 || fraction < 0.0 {
        scaled.ceil()
    } else {
        return height;
    };
    snapped * config.height_snap_interval_metres + config.height_snap_base_metres
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_snap_ties_go_toward_the_authored_base() {
        let config = openzt2_game_data::terrain::TerrainWaterEditingConfiguration {
            height_snap_interval_metres: 0.5,
            height_snap_base_metres: 13.0,
            plane_merge_tolerance_metres: 0.5,
        };
        for (height, expected) in [
            (13.25, 13.0),
            (12.75, 13.0),
            (13.26, 13.5),
            (12.74, 12.5),
            (14.0, 14.0),
        ] {
            assert_eq!(snap_water_height(height, config), expected);
        }
        assert_eq!(
            snap_water_height(
                13.26,
                openzt2_game_data::terrain::TerrainWaterEditingConfiguration {
                    height_snap_interval_metres: 0.0,
                    ..config
                }
            ),
            13.26
        );
    }
}

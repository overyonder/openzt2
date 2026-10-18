use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::AssetId;

/// Mutable water facts owned by a region entity and changed by terrain edits.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TerrainWaterRegionEdit {
    pub terrain: Handle<TerrainAsset>,
    pub biome_index: usize,
    pub surface_height_metres: f32,
    pub depth_ratio: f32,
    pub uses_tank_minimum_depth: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainWaterRegionEditDelta {
    pub owner: Entity,
    pub before: Option<TerrainWaterRegionEdit>,
    pub after: Option<TerrainWaterRegionEdit>,
    pub before_active: bool,
    pub after_active: bool,
}

#[derive(Component, Debug, Default)]
pub struct TerrainWaterRegionRetired;

#[derive(SystemParam)]
pub struct TerrainWaterRegionQuery<'w, 's> {
    regions: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static TerrainWaterRegionSource>,
            Option<&'static TerrainWaterRegionEdit>,
        ),
        (
            Or<(With<TerrainWaterRegionSource>, With<TerrainWaterRegionEdit>)>,
            Without<TerrainWaterRegionRetired>,
        ),
    >,
}

pub struct TerrainWaterRegionView<'a> {
    source: Option<&'a TerrainWaterRegionSource>,
    edit: Option<&'a TerrainWaterRegionEdit>,
}

impl TerrainWaterRegionQuery<'_, '_> {
    /// Revision of the borrowed source facts used by a terrain's derived index.
    pub fn source_revision(&self, terrain: bevy::asset::AssetId<TerrainAsset>) -> u64 {
        use std::hash::{Hash, Hasher};
        self.iter()
            .filter(|(_, view)| view.terrain().id() == terrain)
            .fold(0_u64, |revision, (owner, view)| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                owner.hash(&mut hash);
                view.source
                    .map(TerrainWaterRegionSource::source_region_index)
                    .hash(&mut hash);
                if let Some(edit) = view.edit {
                    edit.biome_index.hash(&mut hash);
                    edit.surface_height_metres.to_bits().hash(&mut hash);
                    edit.depth_ratio.to_bits().hash(&mut hash);
                    edit.uses_tank_minimum_depth.hash(&mut hash);
                }
                revision.wrapping_add(hash.finish())
            })
    }
    pub fn get(&self, owner: Entity) -> Option<TerrainWaterRegionView<'_>> {
        let (_, source, edit) = self.regions.get(owner).ok()?;
        (source.is_some() || edit.is_some()).then_some(TerrainWaterRegionView { source, edit })
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, TerrainWaterRegionView<'_>)> {
        self.regions
            .iter()
            .map(|(owner, source, edit)| (owner, TerrainWaterRegionView { source, edit }))
    }

    pub fn resolve_affiliation(
        &self,
        terrain: bevy::asset::AssetId<TerrainAsset>,
        affiliation: crate::terrain::terrain_edit_data_types::TerrainWaterAffiliation,
    ) -> Option<Entity> {
        use crate::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
        match affiliation {
            TerrainWaterAffiliation::Edited(owner) => {
                (self.get(owner)?.terrain().id() == terrain).then_some(owner)
            }
            TerrainWaterAffiliation::SourceRow(row) => {
                self.regions.iter().find_map(|(owner, source, _)| {
                    let source = source?;
                    (source.terrain().id() == terrain
                        && source.source_region_index() == row as usize)
                        .then_some(owner)
                })
            }
            TerrainWaterAffiliation::Unresolved => None,
        }
    }
}

impl TerrainWaterRegionView<'_> {
    pub fn uses_tank_minimum_depth(&self, terrain: &TerrainAsset) -> Option<bool> {
        self.edit
            .map(|edit| edit.uses_tank_minimum_depth)
            .or_else(|| {
                Some(
                    terrain
                        .canonical_terrain_grid()
                        .water_regions
                        .get(self.source?.source_region_index())?
                        .uses_tank_minimum_depth,
                )
            })
    }
    pub fn edited_state(&self) -> Option<&TerrainWaterRegionEdit> {
        self.edit
    }
    pub fn terrain(&self) -> &Handle<TerrainAsset> {
        self.edit.map(|edit| &edit.terrain).unwrap_or_else(|| {
            self.source
                .expect("region has a source or an edit")
                .terrain()
        })
    }

    pub fn surface_height(&self, terrain: &TerrainAsset) -> Option<f32> {
        self.edit
            .map(|edit| edit.surface_height_metres)
            .or_else(|| self.source?.surface_height(terrain))
    }

    pub fn biome<'a>(
        &self,
        terrain: &'a TerrainAsset,
    ) -> Option<&'a openzt2_game_data::terrain::TerrainBiome> {
        if let Some(edit) = self.edit {
            terrain
                .canonical_terrain_grid()
                .biomes
                .get(edit.biome_index)
        } else {
            terrain.water_biome_for_region(self.source?.source_region_index())
        }
    }

    pub fn depth_ratio(&self, terrain: &TerrainAsset) -> Option<f32> {
        self.edit.map(|edit| edit.depth_ratio).or_else(|| {
            Some(
                terrain
                    .canonical_terrain_grid()
                    .water_regions
                    .get(self.source?.source_region_index())?
                    .depth_ratio,
            )
        })
    }
}

/// Binds an existing region entity to its immutable terrain source row.
#[derive(Component, Debug)]
pub struct TerrainWaterRegionSource {
    terrain: Handle<TerrainAsset>,
    source_region_index: usize,
}

impl TerrainWaterRegionSource {
    pub fn new(terrain: Handle<TerrainAsset>, source_region_index: usize) -> Self {
        Self {
            terrain,
            source_region_index,
        }
    }

    pub fn terrain(&self) -> &Handle<TerrainAsset> {
        &self.terrain
    }
    pub fn source_region_index(&self) -> usize {
        self.source_region_index
    }

    pub fn surface_height(&self, terrain: &TerrainAsset) -> Option<f32> {
        let grid = terrain.canonical_terrain_grid();
        Some(
            grid.units.base_height_metres
                + grid
                    .water_regions
                    .get(self.source_region_index)?
                    .height_relative_to_base_metres,
        )
    }
}

/// Derived spatial index; removed when terrain edits invalidate its source.
#[derive(Component, Debug)]
pub struct TerrainWaterMembershipIndex {
    terrain: bevy::asset::AssetId<TerrainAsset>,
    width: u32,
    height: u32,
    point_owners: Box<[Option<Entity>]>,
    side_owners: Box<[[Option<Entity>; 4]]>,
    regions: Box<[TerrainWaterRegionSpatialIndex]>,
    empty_selector_topology: bool,
    selector_projection: Box<[Option<[u8; 4]>]>,
    fence_probes: Box<[[Box<[[Vec2; 2]]>; 8]]>,
    height_support_limits: Box<[[f32; 2]]>,
    region_source_revision: u64,
    tank_minimum_depth: Option<f32>,
    edited_point_heights: Option<Box<[f32]>>,
}

#[derive(Debug)]
struct TerrainWaterRegionSpatialIndex {
    owner: Entity,
    points: Box<[UVec2]>,
    tiles: Box<[UVec2]>,
}

impl TerrainWaterMembershipIndex {
    pub fn new(
        terrain: bevy::asset::AssetId<TerrainAsset>,
        width: u32,
        height: u32,
        point_owners: Box<[Option<Entity>]>,
        side_owners: Box<[[Option<Entity>; 4]]>,
        empty_selector_topology: bool,
    ) -> Option<Self> {
        let points = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        let tiles = usize::try_from(width.checked_sub(1)? / 4)
            .ok()?
            .checked_mul(usize::try_from(height.checked_sub(1)? / 4).ok()?)?;
        if point_owners.len() != points || side_owners.len() != tiles {
            return None;
        }
        let mut grouped = std::collections::BTreeMap::<Entity, (Vec<UVec2>, Vec<UVec2>)>::new();
        let tile_width = (width - 1) / 4;
        let tile_height = (height - 1) / 4;
        for (index, owner) in point_owners.iter().enumerate() {
            let Some(owner) = *owner else {
                continue;
            };
            let point = UVec2::new(index as u32 % width, index as u32 / width);
            let entry = grouped.entry(owner).or_default();
            entry.0.push(point);
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
                    if x < tile_width && y < tile_height {
                        entry.1.push(UVec2::new(x, y));
                    }
                }
            }
        }
        let regions = grouped
            .into_iter()
            .map(|(owner, (points, mut tiles))| {
                tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
                tiles.dedup();
                TerrainWaterRegionSpatialIndex {
                    owner,
                    points: points.into_boxed_slice(),
                    tiles: tiles.into_boxed_slice(),
                }
            })
            .collect();
        let point_count = point_owners.len();
        Some(Self {
            terrain,
            width,
            height,
            point_owners,
            side_owners,
            regions,
            empty_selector_topology,
            selector_projection: vec![empty_selector_topology.then_some([0; 4]); tiles]
                .into_boxed_slice(),
            fence_probes: (0..tiles)
                .map(|_| std::array::from_fn(|_| Box::default()))
                .collect(),
            height_support_limits: vec![[-1000.0, 1000.0]; point_count].into_boxed_slice(),
            region_source_revision: 0,
            tank_minimum_depth: None,
            edited_point_heights: None,
        })
    }

    pub fn with_edited_point_heights(mut self, heights: Box<[f32]>) -> Option<Self> {
        if heights.len() != self.point_owners.len() {
            return None;
        }
        self.edited_point_heights = Some(heights);
        Some(self)
    }

    pub fn empty_selector_topology(&self) -> bool {
        self.empty_selector_topology
    }

    pub fn with_selector_projection(mut self, projection: Box<[Option<[u8; 4]>]>) -> Option<Self> {
        if projection.len() != self.side_owners.len() {
            return None;
        }
        self.selector_projection = projection;
        Some(self)
    }

    pub fn selector_projection_matches(&self, projection: &[Option<[u8; 4]>]) -> bool {
        self.selector_projection.as_ref() == projection
    }

    pub fn with_fence_probes(mut self, probes: Box<[[Box<[[Vec2; 2]]>; 8]]>) -> Option<Self> {
        if probes.len() != self.side_owners.len() {
            return None;
        }
        self.fence_probes = probes;
        Some(self)
    }

    pub fn fence_probes_match(&self, probes: &[[Box<[[Vec2; 2]]>; 8]]) -> bool {
        self.fence_probes.as_ref() == probes
    }

    pub fn with_height_support_limits(mut self, limits: Box<[[f32; 2]]>) -> Option<Self> {
        if limits.len() != self.point_owners.len() {
            return None;
        }
        self.height_support_limits = limits;
        Some(self)
    }

    pub fn height_support_limits_match(&self, limits: &[[f32; 2]]) -> bool {
        self.height_support_limits.as_ref() == limits
    }

    pub fn with_region_source_revision(mut self, revision: u64) -> Self {
        self.region_source_revision = revision;
        self
    }

    pub fn region_source_revision(&self) -> u64 {
        self.region_source_revision
    }

    pub fn with_tank_minimum_depth(mut self, depth: Option<f32>) -> Self {
        self.tank_minimum_depth = depth;
        self
    }

    pub fn tank_minimum_depth(&self) -> Option<f32> {
        self.tank_minimum_depth
    }

    pub fn supports_water_interval(
        &self,
        point: UVec2,
        low: f32,
        high: f32,
        terrain: &TerrainAsset,
    ) -> bool {
        if point.x >= self.width || point.y >= self.height {
            return false;
        }
        let index = (point.y * self.width + point.x) as usize;
        Self::height_limits_support_water_interval(
            self.height_support_limits[index],
            point,
            low,
            high,
            terrain,
        )
    }

    pub fn height_limits_support_water_interval(
        bounds: [f32; 2],
        point: UVec2,
        low: f32,
        high: f32,
        terrain: &TerrainAsset,
    ) -> bool {
        let grid = terrain.canonical_terrain_grid();
        if point.x >= grid.width || point.y >= grid.height {
            return false;
        }
        let Some(sample) = grid.samples.get((point.y * grid.width + point.x) as usize) else {
            return false;
        };
        if sample.water_surface_flag {
            return true;
        }
        if sample.excludes_elevated_path {
            return false;
        }
        let [minimum, maximum] = bounds;
        let floor = minimum.max(sample.support_floor_metres.unwrap_or(-1000.0));
        [low - 0.0005, low + 0.0005, high - 0.0005, high + 0.0005]
            .into_iter()
            .all(|height| {
                height <= maximum && (height >= minimum || floor == grid.units.base_height_metres)
            })
    }

    pub fn with_selector_projection_from(mut self, source: &Self) -> Option<Self> {
        if self.terrain != source.terrain
            || self.width != source.width
            || self.height != source.height
        {
            return None;
        }
        self.selector_projection
            .clone_from(&source.selector_projection);
        self.fence_probes.clone_from(&source.fence_probes);
        self.height_support_limits
            .clone_from(&source.height_support_limits);
        self.region_source_revision = source.region_source_revision;
        self.tank_minimum_depth = source.tank_minimum_depth;
        Some(self)
    }

    pub fn selector_codes(&self, tile: UVec2) -> Option<&[u8; 4]> {
        self.side_owners(tile)?;
        self.selector_projection[(tile.y * ((self.width - 1) / 4) + tile.x) as usize].as_ref()
    }

    pub fn source_point_height(&self, point: UVec2, terrain: &TerrainAsset) -> Option<f32> {
        if point.x >= self.width || point.y >= self.height {
            return None;
        }
        if let Some(heights) = &self.edited_point_heights {
            return heights
                .get((point.y * self.width + point.x) as usize)
                .copied();
        }
        let grid = terrain.canonical_terrain_grid();
        Some(
            grid.units.base_height_metres
                + grid
                    .samples
                    .get((point.y * self.width + point.x) as usize)?
                    .height_relative_to_base_metres,
        )
    }

    pub fn slope_tile_center_height(&self, tile: UVec2, terrain: &TerrainAsset) -> Option<f32> {
        self.side_owners(tile)?;
        self.source_point_height(tile * 4 + UVec2::splat(2), terrain)
    }

    pub fn terrain(&self) -> bevy::asset::AssetId<TerrainAsset> {
        self.terrain
    }

    pub fn accepted_points(&self, owner: Entity) -> impl Iterator<Item = UVec2> + '_ {
        self.regions
            .binary_search_by_key(&owner, |region| region.owner)
            .ok()
            .into_iter()
            .flat_map(|index| self.regions[index].points.iter().copied())
    }

    pub fn render_tiles(&self, owner: Entity) -> impl Iterator<Item = UVec2> + '_ {
        self.regions
            .binary_search_by_key(&owner, |region| region.owner)
            .ok()
            .into_iter()
            .flat_map(|index| self.regions[index].tiles.iter().copied())
    }

    pub fn render_area_m2(&self, owner: Entity, terrain: &TerrainAsset) -> f32 {
        self.render_tiles(owner).count() as f32
            * (terrain.canonical_terrain_grid().units.cell_size_metres * 4.0).powi(2)
    }

    pub fn point_owner(&self, point: UVec2) -> Option<Entity> {
        if point.x >= self.width || point.y >= self.height {
            return None;
        }
        self.point_owners[(point.y * self.width + point.x) as usize]
    }

    /// The outer None requires the terrain edge/deformation query to resolve.
    pub fn owner_at_world_xz(&self, world: Vec2, terrain: &TerrainAsset) -> Option<Option<Entity>> {
        let spacing = terrain.canonical_terrain_grid().units.cell_size_metres;
        let source = Vec2::new(world.x, -world.y) / spacing;
        if !source.is_finite()
            || source.min_element() < 0.0
            || source.x >= (self.width - 1) as f32
            || source.y >= (self.height - 1) as f32
        {
            return Some(None);
        }
        let tile = source.as_uvec2() / 4;
        let slots = self.side_owners(tile)?;
        let offset = source - (tile * 4 + UVec2::splat(2)).as_vec2();
        let quadrant = usize::from(offset.y < offset.x) * 2 + usize::from(-offset.y < offset.x);
        let side = [3, 0, 2, 1][quadrant];
        let owner = slots[side];
        let neighbour = match side {
            0 => Some(tile + UVec2::Y),
            1 => Some(tile + UVec2::X),
            2 => tile.y.checked_sub(1).map(|y| UVec2::new(tile.x, y)),
            3 => tile.x.checked_sub(1).map(|x| UVec2::new(x, tile.y)),
            _ => unreachable!(),
        };
        if slots[(side + 1) % 4] == owner
            && slots[(side + 3) % 4] == owner
            && neighbour
                .and_then(|tile| self.side_owners(tile))
                .is_none_or(|neighbour| neighbour[(side + 2) % 4] == owner)
        {
            return Some(owner);
        }
        self.selector_codes(tile)?;
        let cardinal = if offset.y > offset.x.abs() {
            0
        } else if offset.y <= -offset.x.abs() {
            2
        } else if offset.x > offset.y.abs() {
            1
        } else {
            3
        };
        let probes = &self.fence_probes[(tile.y * ((self.width - 1) / 4) + tile.x) as usize];
        let position = Vec2::new(world.x, -world.y);
        let origin = (tile * 4).as_vec2() * spacing;
        let centre = origin + Vec2::splat(2.0 * spacing);
        let intersects = |start: Vec2, direction: usize| {
            probes[direction].iter().any(|segment| {
                let ray = position - start;
                let edge = segment[1] - segment[0];
                let denominator = ray.perp_dot(edge);
                if f64::from(denominator.abs()) < 0.0001 {
                    return false;
                }
                let difference = segment[0] - start;
                let distance = difference.perp_dot(edge) / denominator;
                let along_edge = difference.perp_dot(ray) / denominator;
                distance > 0.0 && distance < 1.0 && (0.0..=1.0).contains(&along_edge)
            })
        };
        if intersects(centre, cardinal * 2) {
            let neighbour =
                tile.as_ivec2() + [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X][cardinal];
            if neighbour.min_element() >= 0 {
                if let Some(slots) = self.side_owners(neighbour.as_uvec2()) {
                    return Some(slots[(cardinal + 2) % 4]);
                }
            }
            return Some(owner);
        }
        let span = 4.0 * spacing;
        let previous = origin
            + [
                Vec2::splat(span),
                Vec2::new(span, 0.0),
                Vec2::ZERO,
                Vec2::new(0.0, span),
            ][cardinal];
        if intersects(previous, (cardinal * 2 + 7) % 8) {
            return Some(slots[(cardinal + 3) % 4]);
        }
        let next = origin
            + [
                Vec2::new(0.0, span),
                Vec2::splat(span),
                Vec2::new(span, 0.0),
                Vec2::ZERO,
            ][cardinal];
        Some(
            slots[if intersects(next, (cardinal * 2 + 1) % 8) {
                (cardinal + 1) % 4
            } else {
                cardinal
            }],
        )
    }

    pub fn side_owners(&self, tile: UVec2) -> Option<&[Option<Entity>; 4]> {
        let width = (self.width - 1) / 4;
        let height = (self.height - 1) / 4;
        (tile.x < width && tile.y < height)
            .then(|| &self.side_owners[(tile.y * width + tile.x) as usize])
    }

    pub fn tile_owned_by(&self, tile: UVec2, owner: Entity) -> bool {
        self.side_owners(tile)
            .is_some_and(|sides| sides.iter().all(|side| *side == Some(owner)))
    }

    /// Point coordinates in side order, including the shared centre point.
    pub fn side_bucket(tile: UVec2, side: usize) -> Option<[UVec2; 9]> {
        let origin = tile * 4;
        let local = match side {
            0 => [
                UVec2::new(0, 4),
                UVec2::new(1, 4),
                UVec2::new(2, 4),
                UVec2::new(3, 4),
                UVec2::new(4, 4),
                UVec2::new(1, 3),
                UVec2::new(2, 3),
                UVec2::new(3, 3),
                UVec2::new(2, 2),
            ],
            1 => [
                UVec2::new(4, 0),
                UVec2::new(4, 1),
                UVec2::new(4, 2),
                UVec2::new(4, 3),
                UVec2::new(4, 4),
                UVec2::new(3, 1),
                UVec2::new(3, 2),
                UVec2::new(3, 3),
                UVec2::new(2, 2),
            ],
            2 => [
                UVec2::new(0, 0),
                UVec2::new(1, 0),
                UVec2::new(2, 0),
                UVec2::new(3, 0),
                UVec2::new(4, 0),
                UVec2::new(1, 1),
                UVec2::new(2, 1),
                UVec2::new(3, 1),
                UVec2::new(2, 2),
            ],
            3 => [
                UVec2::new(0, 0),
                UVec2::new(0, 1),
                UVec2::new(0, 2),
                UVec2::new(0, 3),
                UVec2::new(0, 4),
                UVec2::new(1, 1),
                UVec2::new(1, 2),
                UVec2::new(1, 3),
                UVec2::new(2, 2),
            ],
            _ => return None,
        };
        Some(local.map(|point| origin + point))
    }
}

/// Last terrain revision applied to the water meshes.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainWaterRevision(pub u64);

/// One material-homogeneous Bevy water mesh parented to its terrain chunk.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainWaterSurface;

/// A vertical water strip between two different surface heights.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainWaterfallSurface;

/// A connected water region from the terrain data. Its transform places the
/// spatial-audio emitter at the region's centroid.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct NaturalWaterRegion {
    /// Stable identity of the loaded water style referenced by this region.
    pub style: AssetId,
    pub area_m2: f32,
}

/// Canonical water-cell centres belonging to this region, including surface height.
#[derive(Component, Debug)]
pub struct NaturalWaterRegionCells {
    pub cell_size_m: f32,
    pub centres: Box<[Vec3]>,
}

/// Marks a chunk after spawning the water regions that begin in it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainWaterRegionsProjected;

use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;
use bevy::{ecs::{entity::{EntityHashMap, EntityHashSet}, system::SystemParam}};
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    unconstrained_terrain_water_height, TerrainWaterSupportLimits,
};
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_zoo_state::topology::topology_graph_types::{FenceSegment, PathTile, TileSurface, TopologyGrid, TopologyNode};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackRoute;

/// Borrows live structures; the resulting point limits are a derived terrain index.
#[derive(SystemParam)]
pub(in crate::terrain) struct TerrainHeightSupports<'w, 's> {
    paths: Query<'w, 's, (Entity, &'static PathTile, Option<&'static TileSurface>)>,
    tanks: Query<
        'w,
        's,
        (
            Entity,
            &'static TankGeometry,
            Option<&'static HabitatRegion>,
        ),
    >,
    boundaries: HabitatBoundaryQueries<'w, 's>,
    fences: Query<'w, 's, FenceSegment>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    tracks: Query<'w, 's, (Entity, TrackRoute)>,
    objects: Query<'w, 's, &'static DefinitionId>,
    scene_sources: Query<'w, 's, Ref<'static, PrefabSourceAssetHandle>>,
    scene_footprints: Option<Res<'w, ScenePlacementFootprints>>,
    parents: Query<'w, 's, &'static ChildOf>,
    transforms: TransformHelper<'w, 's>,
}

impl TerrainHeightSupports<'_, '_> {
    pub(super) fn affected_habitats<'a>(
        &'a self,
        fences: &'a EntityHashSet,
    ) -> impl Iterator<Item = Entity> + 'a {
        fences
            .iter()
            .flat_map(|&fence| self.boundaries.habitats(fence))
    }

    pub(super) fn scene_asset_changed(&self, entity: Entity) -> bool {
        self.scene_footprints
            .as_ref()
            .is_some_and(|footprints| footprints.is_changed())
            || self
                .scene_sources
                .get(entity)
                .is_ok_and(|source| source.is_changed())
    }

    pub(super) fn track_entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.tracks.iter().map(|(entity, _)| entity)
    }

    fn endpoint_owner(
        &self,
        mut entity: Entity,
        dependencies: &mut EntityHashSet,
    ) -> Option<(Entity, &DefinitionId)> {
        loop {
            dependencies.insert(entity);
            if let Ok(definition) = self.objects.get(entity) {
                return Some((entity, definition));
            }
            entity = self.parents.get(entity).ok()?.parent();
        }
    }

    fn sky_track(&self, entity: Entity, definitions: WorldDefinitionsView<'_>) -> Option<SkyTrack> {
        let mut source = SkyTrack::default();
        let Ok((_, track)) = self.tracks.get(entity) else {
            return Some(source);
        };
        let definition = definitions.find_track(track.definition)?;
        if definition.kind == TransportationTrackKind::Sky {
            for endpoint in [track.from(), track.to()] {
                if let Some((owner, _)) = self.endpoint_owner(endpoint, &mut source.dependencies) {
                    source
                        .dependencies
                        .extend(self.parents.iter_ancestors::<ChildOf>(owner));
                    source.owners.insert(owner);
                }
            }
        }
        Some(source)
    }

    pub(super) fn contributors(&self) -> impl Iterator<Item = Entity> + '_ {
        self.paths
            .iter()
            .map(|(entity, ..)| entity)
            .chain(self.tanks.iter().map(|(entity, ..)| entity))
    }

    fn contribution(
        &self,
        entity: Entity,
        asset: &TerrainAsset,
        definitions: WorldDefinitionsView<'_>,
        topology: &TopologyGrid,
        sky_supports: &TerrainWaterSkySupports,
    ) -> Option<TerrainWaterSupportLimits> {
        let grid = asset.canonical_terrain_grid();
        let spacing = grid.units.cell_size_metres;
        let mut limits = TerrainWaterSupportLimits::default();
        if let Ok((_, path, surface)) = self.paths.get(entity) {
            (|| {
                let definition = definitions.find_path(path.definition)?;
                if !definition.elevated {
                    return Some(());
                }
                let heights =
                    surface.map_or(definition.surface.height_cm, |surface| surface.height_cm);
                let centre = topology.cell_translation(path.cell);
                let half_width = f32::from(definition.width_cm) * 0.005;
                let corners = [
                    Vec2::new(-half_width, -half_width),
                    Vec2::new(half_width, -half_width),
                    Vec2::new(half_width, half_width),
                    Vec2::new(-half_width, half_width),
                ];
                // The four side fits each constrain their own triangle, including its centre.
                for side in 0..4 {
                    let cap = centre.y
                        + f32::from(heights[side].min(heights[(side + 1) % 4]).min(heights[4]))
                            * 0.01;
                    let a = centre.xz() + corners[side];
                    let b = centre.xz() + corners[(side + 1) % 4];
                    let min = a.min(b).min(centre.xz());
                    let max = a.max(b).max(centre.xz());
                    // Restrict the rectangular iteration to the side triangle below.
                    let Some((minimum, maximum)) =
                        point_rectangle(min, max, spacing, UVec2::new(grid.width, grid.height))
                    else {
                        continue;
                    };
                    for y in minimum.y..=maximum.y {
                        for x in minimum.x..=maximum.x {
                            let p = Vec2::new(x as f32 * spacing, -(y as f32) * spacing);
                            let cross = [
                                (b - a).perp_dot(p - a),
                                (centre.xz() - b).perp_dot(p - b),
                                (a - centre.xz()).perp_dot(p - centre.xz()),
                            ];
                            if cross.iter().all(|value| *value >= 0.0)
                                || cross.iter().all(|value| *value <= 0.0)
                            {
                                let bounds = limits
                                    .entry(y * grid.width + x)
                                    .or_insert_with(unconstrained_terrain_water_height);
                                bounds[1] = bounds[1].min(cap);
                            }
                        }
                    }
                }
                Some(())
            })()?;
        }
        if let Ok((_, tank, habitat)) = self.tanks.get(entity) {
            let habitat = habitat?;
            for cell in &habitat.topology_cells {
                let minimum = topology.cell_translation(cell.extend(0)).xz();
                apply_rectangle(
                    &mut limits,
                    asset,
                    minimum,
                    minimum + Vec2::splat(topology.spacing_m),
                    tank.floor_height,
                    1000.0,
                );
            }
            for fence in self.boundaries.fences(entity) {
                let edge = self.fences.get(fence).ok()?;
                let first = topology
                    .cell_translation(self.nodes.get(edge.a()).ok()?.cell)
                    .xz();
                let second = topology
                    .cell_translation(self.nodes.get(edge.b()).ok()?.cell)
                    .xz();
                let delta = second - first;
                let steps = (delta.abs().max_element() / spacing).round() as u32;
                for step in 0..=steps {
                    let point = first.lerp(second, step as f32 / steps.max(1) as f32);
                    apply_rectangle(&mut limits, asset, point, point, -1000.0, tank.wall_height);
                }
            }
        }
        if sky_supports.owners.contains_key(&entity) {
            let mut cells = Vec::new();
            let definition = definitions.find_placeable(self.objects.get(entity).ok()?.0)?;
            let transform = self
                .transforms
                .compute_global_transform(entity)
                .ok()?
                .compute_transform();
            let source = self.scene_sources.get(entity).ok();
            let geometry = self
                .scene_footprints
                .as_ref()
                .map_or(definition.into(), |footprints| {
                    footprints.resolve(definition, source.as_ref().map(|source| &source.0))
                });
            for cell in
                collect_occupied_placement_cells_for_transform(geometry, &transform, &mut cells)?
            {
                apply_rectangle(
                    &mut limits,
                    asset,
                    cell.as_vec2(),
                    cell.as_vec2() + Vec2::ONE,
                    transform.translation.y,
                    1000.0,
                );
            }
        }
        Some(limits)
    }
}

#[derive(Default)]
struct SkyTrack {
    dependencies: EntityHashSet,
    owners: EntityHashSet,
}

#[derive(Component, Default)]
pub(in crate::terrain) struct TerrainWaterSkySupports {
    tracks: EntityHashMap<SkyTrack>,
    by_dependency: EntityHashMap<EntityHashSet>,
    owners: EntityHashMap<usize>,
    unresolved: EntityHashSet,
}

impl TerrainWaterSkySupports {
    pub(super) fn ready(&self) -> bool {
        self.unresolved.is_empty()
    }

    pub(super) fn owners(&self) -> impl Iterator<Item = Entity> + '_ {
        self.owners.keys().copied()
    }

    pub(super) fn affected_tracks(&self, mut changed: impl FnMut(Entity) -> bool) -> EntityHashSet {
        self.by_dependency
            .iter()
            .filter(move |(entity, _)| changed(**entity))
            .flat_map(|(_, tracks)| tracks.iter().copied())
            .collect()
    }

    pub(super) fn patch(
        &mut self,
        changed: EntityHashSet,
        sources: &TerrainHeightSupports,
        definitions: WorldDefinitionsView<'_>,
        supports: &mut EntityHashSet,
    ) {
        for track in changed {
            if let Some(previous) = self.tracks.remove(&track) {
                for owner in previous.owners {
                    supports.insert(owner);
                    if let bevy::platform::collections::hash_map::Entry::Occupied(mut active) =
                        self.owners.entry(owner)
                    {
                        *active.get_mut() -= 1;
                        if *active.get() == 0 {
                            active.remove();
                        }
                    }
                }
                for dependency in previous.dependencies {
                    if let bevy::platform::collections::hash_map::Entry::Occupied(mut active) =
                        self.by_dependency.entry(dependency)
                    {
                        active.get_mut().remove(&track);
                        if active.get().is_empty() {
                            active.remove();
                        }
                    }
                }
            }
            self.unresolved.remove(&track);
            if let Some(source) = sources.sky_track(track, definitions) {
                if !source.dependencies.is_empty() {
                    for dependency in source.dependencies.iter().copied() {
                        self.by_dependency
                            .entry(dependency)
                            .or_default()
                            .insert(track);
                    }
                    for owner in source.owners.iter().copied() {
                        supports.insert(owner);
                        *self.owners.entry(owner).or_default() += 1;
                    }
                    self.tracks.insert(track, source);
                }
            } else {
                self.unresolved.insert(track);
            }
        }
    }
}

#[derive(Component, Default)]
pub(in crate::terrain) struct TerrainWaterHeightLimits {
    by_source: EntityHashMap<Box<[u32]>>,
    by_point: bevy::platform::collections::HashMap<u32, EntityHashMap<[f32; 2]>>,
    pub(super) limits: TerrainWaterSupportLimits,
}

impl TerrainWaterHeightLimits {
    pub(super) fn patch(
        &mut self,
        sources: &TerrainHeightSupports,
        changed_sources: impl IntoIterator<Item = Entity>,
        asset: &TerrainAsset,
        definitions: WorldDefinitionsView<'_>,
        topology: &TopologyGrid,
        sky: &TerrainWaterSkySupports,
    ) -> Option<Vec<(u32, Option<[f32; 2]>)>> {
        let contributions = changed_sources
            .into_iter()
            .map(|entity| {
                Some((
                    entity,
                    sources.contribution(entity, asset, definitions, topology, sky)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let mut dirty = bevy::platform::collections::HashSet::<u32>::default();
        for (entity, next) in contributions {
            if let Some(previous) = self.by_source.remove(&entity) {
                for point in previous.iter().copied() {
                    if let Some(contributors) = self.by_point.get_mut(&point) {
                        contributors.remove(&entity);
                    }
                    dirty.insert(point);
                }
            }
            if !next.is_empty() {
                self.by_source
                    .insert(entity, next.keys().copied().collect());
            }
            for (point, bounds) in next {
                self.by_point
                    .entry(point)
                    .or_default()
                    .insert(entity, bounds);
                dirty.insert(point);
            }
        }
        let mut patch = Vec::new();
        for point in dirty {
            let bounds = self
                .by_point
                .get(&point)
                .filter(|sources| !sources.is_empty())
                .map(|sources| {
                    sources
                        .values()
                        .fold(unconstrained_terrain_water_height(), |bounds, source| {
                            [bounds[0].max(source[0]), bounds[1].min(source[1])]
                        })
                });
            if bounds.is_none() {
                self.by_point.remove(&point);
            }
            if self.limits.get(&point).copied() != bounds {
                if let Some(bounds) = bounds {
                    self.limits.insert(point, bounds);
                } else {
                    self.limits.remove(&point);
                }
                patch.push((point, bounds));
            }
        }
        Some(patch)
    }
}

fn point_rectangle(
    minimum: Vec2,
    maximum: Vec2,
    spacing: f32,
    dimensions: UVec2,
) -> Option<(UVec2, UVec2)> {
    let minimum_point = (Vec2::new(minimum.x, -maximum.y) / spacing)
        .ceil()
        .max(Vec2::ZERO);
    let maximum_point = (Vec2::new(maximum.x, -minimum.y) / spacing)
        .floor()
        .min((dimensions - UVec2::ONE).as_vec2());
    maximum_point
        .cmpge(minimum_point)
        .all()
        .then(|| (minimum_point.as_uvec2(), maximum_point.as_uvec2()))
}

fn apply_rectangle(
    limits: &mut TerrainWaterSupportLimits,
    asset: &TerrainAsset,
    minimum: Vec2,
    maximum: Vec2,
    lower: f32,
    upper: f32,
) {
    let grid = asset.canonical_terrain_grid();
    let Some((minimum, maximum)) = point_rectangle(
        minimum,
        maximum,
        grid.units.cell_size_metres,
        UVec2::new(grid.width, grid.height),
    ) else {
        return;
    };
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let bounds = limits
                .entry(y * grid.width + x)
                .or_insert_with(unconstrained_terrain_water_height);
            bounds[0] = bounds[0].max(lower);
            bounds[1] = bounds[1].min(upper);
        }
    }
}

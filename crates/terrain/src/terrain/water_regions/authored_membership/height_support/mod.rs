use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, PathTile, TileSurface, TopologyGrid, TopologyNode};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;

/// Borrows live structures; the resulting point limits are a derived terrain index.
#[derive(SystemParam)]
pub(in crate::terrain) struct TerrainHeightSupports<'w, 's> {
    paths: Query<'w, 's, (&'static PathTile, Option<&'static TileSurface>)>,
    tanks: Query<'w, 's, (&'static TankGeometry, Option<&'static HabitatRegion>)>,
    fences: Query<'w, 's, &'static FenceEdge>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    tracks: Query<'w, 's, &'static TrackSegment>,
    objects: Query<'w, 's, &'static DefinitionId>,
    parents: Query<'w, 's, &'static ChildOf>,
    transforms: TransformHelper<'w, 's>,
}

impl TerrainHeightSupports<'_, '_> {
    fn endpoint_owner(&self, mut entity: Entity) -> Option<(Entity, &DefinitionId)> {
        loop {
            if let Ok(definition) = self.objects.get(entity) {
                return Some((entity, definition));
            }
            entity = self.parents.get(entity).ok()?.parent();
        }
    }

    pub(super) fn sky_supports(
        &self,
        definitions: WorldDefinitionsView<'_>,
    ) -> Option<Vec<(Entity, openzt2_game_data::AssetId, Option<Transform>)>> {
        let mut owners = Vec::new();
        for track in &self.tracks {
            if definitions.find_track(track.definition)?.kind == TransportationTrackKind::Sky {
                owners.extend([track.from, track.to].into_iter().filter_map(|endpoint| {
                    let (entity, definition) = self.endpoint_owner(endpoint)?;
                    Some((entity, definition.0))
                }));
            }
        }
        owners.sort_unstable_by_key(|(entity, _)| *entity);
        owners.dedup_by_key(|(entity, _)| *entity);
        Some(
            owners
                .into_iter()
                .map(|(entity, definition)| {
                    (
                        entity,
                        definition,
                        self.transforms
                            .compute_global_transform(entity)
                            .ok()
                            .map(|value| value.compute_transform()),
                    )
                })
                .collect(),
        )
    }

    pub(super) fn point_limits(
        &self,
        asset: &TerrainAsset,
        definitions: WorldDefinitionsView<'_>,
        topology: &TopologyGrid,
        sky_supports: &[(Entity, openzt2_game_data::AssetId, Option<Transform>)],
    ) -> Option<Box<[[f32; 2]]>> {
        let grid = asset.canonical_terrain_grid();
        let spacing = grid.units.cell_size_metres;
        let mut limits = vec![[-1000.0_f32, 1000.0_f32]; grid.samples.len()];
        for (path, surface) in &self.paths {
            let definition = definitions.find_path(path.definition)?;
            if !definition.elevated {
                continue;
            }
            let heights = surface.map_or(definition.surface.height_cm, |surface| surface.height_cm);
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
                    + f32::from(heights[side].min(heights[(side + 1) % 4]).min(heights[4])) * 0.01;
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
                            let bounds = &mut limits[(y * grid.width + x) as usize];
                            bounds[1] = bounds[1].min(cap);
                        }
                    }
                }
            }
        }
        // A tank floor is owned by the habitat's exact topology cells, not its bounds.
        for (tank, habitat) in &self.tanks {
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
            for edge in &habitat.boundary_fence_entities {
                let edge = self.fences.get(*edge).ok()?;
                let first = topology
                    .cell_translation(self.nodes.get(edge.a).ok()?.cell)
                    .xz();
                let second = topology
                    .cell_translation(self.nodes.get(edge.b).ok()?.cell)
                    .xz();
                let delta = second - first;
                let steps = (delta.abs().max_element() / spacing).round() as u32;
                for step in 0..=steps {
                    let point = first.lerp(second, step as f32 / steps.max(1) as f32);
                    apply_rectangle(&mut limits, asset, point, point, -1000.0, tank.wall_height);
                }
            }
        }
        let mut cells = Vec::new();
        for (_, definition, transform) in sky_supports {
            let definition = definitions.find_placeable(*definition)?;
            let transform = transform.as_ref()?;
            for cell in
                collect_occupied_placement_cells_for_transform(definition, transform, &mut cells)?
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
        Some(limits.into_boxed_slice())
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
    limits: &mut [[f32; 2]],
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
            let bounds = &mut limits[(y * grid.width + x) as usize];
            bounds[0] = bounds[0].max(lower);
            bounds[1] = bounds[1].min(upper);
        }
    }
}

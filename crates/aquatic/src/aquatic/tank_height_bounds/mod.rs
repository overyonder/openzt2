use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWallTopHeight;
use openzt2_zoo_state::aquatic::tank_geometry_edit_types::{TankHeightBounds, TankWallHeightEdit};
use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::topology_graph_types::{
    FenceEdge, PathTile, TopologyGrid, TopologyNode,
};
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_adjacent_topology_grid_cells_between_endpoints;

use super::tank_geometry_rules::TankGeometryLimits;

const ELEVATED_PATH_CLEARANCE_OVER_TANK_WALL_M: f32 = 4.0;

#[derive(SystemParam)]
pub(super) struct TankHeightBoundsQueries<'w, 's> {
    fences: Query<
        'w,
        's,
        (
            &'static FenceEdge,
            &'static PersistentId,
            Option<&'static TankWallTopHeight>,
        ),
    >,
    nodes: Query<'w, 's, &'static TopologyNode>,
    paths: Query<'w, 's, &'static PathTile>,
    grid: Res<'w, TopologyGrid>,
    show_stages: Query<
        'w,
        's,
        &'static Transform,
        With<openzt2_zoo_state::shows::show_stage_types::ShowStage>,
    >,
    ground: TankWallGroundSampling<'w, 's>,
}

impl TankHeightBoundsQueries<'_, '_> {
    pub(super) fn wall_height_images(
        &self,
        region: &HabitatRegion,
        limits: TankGeometryLimits,
    ) -> Option<Box<[TankWallHeightEdit]>> {
        let mut walls = Vec::with_capacity(region.boundary_fence_entities.len());
        for fence in region.boundary_fence_entities.iter() {
            let (edge, id, top) = self.fences.get(*fence).ok()?;
            let (a, b) = (
                self.nodes.get(edge.a).ok()?.cell,
                self.nodes.get(edge.b).ok()?.cell,
            );
            let cells = calculate_adjacent_topology_grid_cells_between_endpoints(a, b).ok()?;
            let mut highest = f32::NEG_INFINITY;
            let mut maximum = f32::INFINITY;
            for cell in &cells {
                highest = highest.max(self.ground.ground_height_at_topology_cell(*cell)?);
                maximum = maximum.min(
                    self.ground
                        .terrain_units_at_topology_cell(*cell)?
                        .maximum_height_metres
                        + limits.default_wall_height_m,
                );
            }
            for path in &self.paths {
                if path.cell.z != 0 && cells.iter().any(|cell| cell.xy() == path.cell.xy()) {
                    maximum =
                        maximum.min(path.cell.z as f32 - ELEVATED_PATH_CLEARANCE_OVER_TANK_WALL_M);
                }
            }
            let before = top.map_or(highest + limits.default_wall_height_m, |top| top.0);
            walls.push(TankWallHeightEdit {
                id: *id,
                before,
                after: before,
                minimum: highest + limits.minimum_wall_height_m,
                maximum,
            });
        }
        walls.sort_unstable_by_key(|wall| wall.id.0);
        (!walls.is_empty()).then(|| walls.into_boxed_slice())
    }

    pub(super) fn include_adjacent_tank_water_in_wall_minima(
        &self,
        walls: &mut [TankWallHeightEdit],
        adjacent_region: &HabitatRegion,
        water_height: f32,
        limits: TankGeometryLimits,
    ) {
        for entity in adjacent_region.boundary_fence_entities.iter() {
            let Ok((_, id, _)) = self.fences.get(*entity) else {
                continue;
            };
            if let Ok(index) = walls.binary_search_by_key(&id.0, |wall| wall.id.0) {
                walls[index].minimum = walls[index]
                    .minimum
                    .max(water_height + limits.water_offset_m);
            }
        }
    }

    pub(super) fn tank_height_bounds(
        &self,
        region: &HabitatRegion,
        limits: TankGeometryLimits,
    ) -> Option<TankHeightBounds> {
        let walls = self.wall_height_images(region, limits)?;
        let (edge, ..) = self
            .fences
            .get(*region.boundary_fence_entities.first()?)
            .ok()?;
        let cell = self.nodes.get(edge.a).ok()?.cell;
        let units = self.ground.terrain_units_at_topology_cell(cell)?;
        let is_show_tank = self.show_stages.iter().any(|transform| {
            let cell = ((transform.translation.xz() - self.grid.origin) / self.grid.spacing_m)
                .floor()
                .as_ivec2();
            region
                .topology_cells
                .binary_search_by_key(&(cell.x, cell.y), |cell| (cell.x, cell.y))
                .is_ok()
        });
        Some(TankHeightBounds {
            minimum_water_depth_m: if is_show_tank {
                limits
                    .minimum_water_depth_m
                    .max(limits.minimum_show_water_depth_m)
            } else {
                limits.minimum_water_depth_m
            },
            water_minimum: walls
                .iter()
                .map(|wall| wall.minimum - limits.minimum_wall_height_m - limits.water_offset_m)
                .fold(f32::NEG_INFINITY, f32::max),
            floor_minimum: units.minimum_water_height_metres.unwrap_or(0.0),
        })
    }
}

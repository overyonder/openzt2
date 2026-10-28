//! Tank height rules: wall tops, water following the rim and
//! floor clearance, shared by tank creation, gestures and edit validation.

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::TankEditKind;

use super::tank_geometry_edit_types::{TankGeometryEdit, TankHeightBounds};

#[derive(Debug, Clone, Copy)]
pub(super) struct TankGeometryLimits {
    /// A full tank's water stands this far below its wall top.
    pub water_offset_m: f32,
    /// A new tank's floor lies this far below the mean ground under its walls.
    pub floor_offset_m: f32,
    pub minimum_water_depth_m: f32,
    pub minimum_show_water_depth_m: f32,
    pub default_wall_height_m: f32,
    pub minimum_wall_height_m: f32,
}

pub(super) fn tank_geometry_limits(
    definitions: WorldDefinitionsView<'_>,
) -> Option<TankGeometryLimits> {
    let surface = definitions.tank_surface_policy()?;
    let depth = definitions.tank_depth_policy()?;
    let fence_mode = definitions.fence_placement_mode_policy()?;
    Some(TankGeometryLimits {
        water_offset_m: surface.water_height_offset_cm as f32 / 100.0,
        floor_offset_m: surface.floor_height_offset_cm as f32 / 100.0,
        minimum_water_depth_m: depth.minimum_water_depth_cm as f32 / 100.0,
        minimum_show_water_depth_m: depth.minimum_show_tank_depth_cm as f32 / 100.0,
        default_wall_height_m: fence_mode.default_tank_wall_height_cm as f32 / 100.0,
        minimum_wall_height_m: fence_mode.minimum_tank_wall_height_cm as f32 / 100.0,
    })
}

/// A new tank: full to its lowest wall top, with its floor below the mean
/// wall ground by the floor offset, never above the lowest wall ground nor
/// below zero.
pub(super) fn new_tank_geometry(
    lowest_wall_top: f32,
    ground: BoundaryGround,
    area: f32,
    limits: TankGeometryLimits,
) -> Option<TankGeometry> {
    let floor_height = (ground.mean - limits.floor_offset_m)
        .min(ground.lowest)
        .max(0.0);
    let mut geometry = TankGeometry {
        floor_height,
        wall_height: lowest_wall_top,
        water_height: lowest_wall_top - limits.water_offset_m,
        water_height_limit: 1_000_000.0,
        wall_base_height: ground.lowest,
        area: area.max(0.0),
        volume: 0.0,
    };
    geometry.volume = geometry.area * geometry.depth();
    geometry.is_valid().then_some(geometry)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BoundaryGround {
    pub lowest: f32,
    pub mean: f32,
}

/// Wall edits refill up to the rim or the player's water ceiling.
pub(super) fn water_height_after_wall_change(
    geometry: TankGeometry,
    limits: TankGeometryLimits,
) -> f32 {
    (geometry.wall_height - limits.water_offset_m).min(geometry.water_height_limit)
}

/// The floor a gesture may request: at least the minimum water depth below
/// the water and never above the lowest ground under the walls, but never
/// below the terrain's minimum water height.
pub(super) fn clamp_floor_height(
    geometry: TankGeometry,
    requested: f32,
    bounds: TankHeightBounds,
) -> f32 {
    let floor = requested
        .min(geometry.water_height - bounds.minimum_water_depth_m)
        .min(geometry.wall_base_height);
    floor.max(bounds.floor_minimum)
}

/// Applies one gesture step of `delta` metres to the edit's working geometry.
pub(super) fn step_tank_geometry_edit(
    edit: &mut TankGeometryEdit,
    delta: f32,
    limits: TankGeometryLimits,
    bounds: TankHeightBounds,
) {
    let after = &mut edit.after;
    match edit.kind {
        TankEditKind::RaiseFloor | TankEditKind::LowerFloor => {
            after.floor_height = clamp_floor_height(*after, after.floor_height + delta, bounds);
        }
        TankEditKind::Fill | TankEditKind::Drain => {
            let full = after.wall_height - limits.water_offset_m;
            let minimum = bounds
                .water_minimum
                .max(after.floor_height + bounds.minimum_water_depth_m);
            after.water_height = (after.water_height + delta).max(minimum).min(full);
            after.water_height_limit = if after.water_height == full {
                1_000_000.0
            } else {
                after.water_height
            };
        }
        TankEditKind::RaiseWall | TankEditKind::LowerWall => {
            let rim = after.water_height + limits.water_offset_m;
            let requested = rim + delta;
            for wall in edit.wall_heights.iter_mut() {
                let in_band = if delta >= 0.0 {
                    wall.after >= rim - f32::EPSILON && wall.after <= requested + f32::EPSILON
                } else {
                    wall.after >= requested - f32::EPSILON && wall.after <= rim + f32::EPSILON
                };
                if in_band {
                    wall.after = requested.max(wall.minimum).min(wall.maximum);
                }
            }
            after.wall_height = edit
                .wall_heights
                .iter()
                .map(|wall| wall.after)
                .fold(f32::INFINITY, f32::min);
            after.water_height = water_height_after_wall_change(*after, limits);
        }
    }
    after.volume = after.area * after.depth();
}

/// Whether `geometry` is a state the edit's gesture rules can produce from
/// its `before` image.
pub(super) fn tank_geometry_edit_reaches(
    edit: &TankGeometryEdit,
    geometry: TankGeometry,
    definitions: WorldDefinitionsView<'_>,
    limits: TankGeometryLimits,
    bounds: TankHeightBounds,
) -> bool {
    let Some(tank) = definitions.find_tank(edit.definition) else {
        return false;
    };
    let before = edit.before;
    let depth_within_definition = geometry.depth() >= f32::from(tank.min_depth_cm) / 100.0
        && geometry.depth() <= f32::from(tank.max_depth_cm) / 100.0;
    let shape_kept =
        geometry.area == before.area && geometry.wall_base_height == before.wall_base_height;
    let water_limit_kept = geometry.water_height_limit == before.water_height_limit;
    let kind_rules = match edit.kind {
        TankEditKind::Fill | TankEditKind::Drain => {
            let full = geometry.wall_height - limits.water_offset_m;
            geometry.floor_height == before.floor_height
                && geometry.wall_height == before.wall_height
                && geometry.water_height
                    == geometry
                        .water_height
                        .max(
                            bounds
                                .water_minimum
                                .max(geometry.floor_height + bounds.minimum_water_depth_m),
                        )
                        .min(full)
                && geometry.water_height_limit
                    == (if geometry.water_height == full {
                        1_000_000.0
                    } else {
                        geometry.water_height
                    })
                && match edit.kind {
                    TankEditKind::Fill => geometry.water_height >= before.water_height,
                    _ => geometry.water_height <= before.water_height,
                }
        }
        TankEditKind::RaiseFloor | TankEditKind::LowerFloor => {
            water_limit_kept
                && geometry.wall_height == before.wall_height
                && geometry.water_height == before.water_height
                && clamp_floor_height(geometry, geometry.floor_height, bounds)
                    == geometry.floor_height
        }
        TankEditKind::RaiseWall | TankEditKind::LowerWall => {
            water_limit_kept
                && geometry.floor_height == before.floor_height
                && !edit.wall_heights.is_empty()
                && edit.wall_heights.iter().all(|wall| {
                    wall.after.is_finite()
                        && (wall.after == wall.before
                            || wall.after == wall.after.max(wall.minimum).min(wall.maximum))
                })
                && geometry.wall_height
                    == edit
                        .wall_heights
                        .iter()
                        .map(|wall| wall.after)
                        .fold(f32::INFINITY, f32::min)
                && geometry.water_height == water_height_after_wall_change(geometry, limits)
        }
    };
    geometry.is_valid()
        && geometry.area > 0.0
        && depth_within_definition
        && shape_kept
        && kind_rules
}

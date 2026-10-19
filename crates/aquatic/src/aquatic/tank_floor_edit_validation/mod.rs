use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{aquatic_simulation_types::TankGeometry, tank_floor_edit_types::TankFloorEdit};

pub(super) fn tank_floor_edit_matches_authored_depth_bounds(
    edit: &TankFloorEdit,
    geometry: TankGeometry,
    definitions: WorldDefinitionsView<'_>,
) -> bool {
    let (Some(tank), Some(depth)) = (
        definitions.find_tank(edit.definition),
        definitions.tank_depth_policy(),
    ) else {
        return false;
    };
    let minimum = u32::from(tank.min_depth_cm).max(depth.minimum_water_depth_cm) as f32 / 100.0;
    let maximum = f32::from(tank.max_depth_cm) / 100.0;
    geometry.is_valid()
        && geometry.area > 0.0
        && geometry.depth() >= minimum
        && geometry.depth() <= maximum
        && edit.before.area == edit.after.area
        // A floor edit keeps the rim and water; a wall edit keeps the floor.
        && ((edit.before.wall_height == edit.after.wall_height
            && edit.before.water_height == edit.after.water_height)
            || edit.before.floor_height == edit.after.floor_height)
}

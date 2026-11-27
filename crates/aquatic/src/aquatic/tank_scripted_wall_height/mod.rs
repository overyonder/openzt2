use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::TankEditKind;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use super::{
    aquatic_simulation_types::{tank_habitat_seed_cell, Tank, TankGeometry},
    tank_geometry_edit_types::{
        ScriptedTankGeometryEdit, SetTankWallHeights, TankGeometryEdit, TankGeometryGesture,
    },
    tank_geometry_rules::{step_tank_geometry_edit, tank_geometry_limits},
    tank_height_bounds::TankHeightBoundsQueries,
};

/// Moves every tank's wall tops to the height a scenario script asks for, as
/// the original's `ZT_SET_TANK_WALL_HEIGHT` does. Each tank's edit is stepped
/// by the same rule the raise and lower wall tools use and committed as an
/// ordinary construction edit.
#[allow(clippy::type_complexity)]
pub(super) fn apply_scripted_tank_wall_heights(
    mut commands: Commands,
    mut requests: MessageReader<SetTankWallHeights>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    tanks: Query<(
        Entity,
        &Tank,
        &TankGeometry,
        &HabitatRegion,
        &HabitatSummary,
        &WorldMember,
    )>,
    height_queries: TankHeightBoundsQueries,
    mut commits: MessageWriter<CommitConstruction>,
) {
    let Some(SetTankWallHeights(target)) = requests.read().last().copied() else {
        return;
    };
    let Some(limits) = active_definitions
        .get(&definitions)
        .and_then(tank_geometry_limits)
    else {
        return;
    };
    for (habitat, tank, geometry, region, summary, member) in &tanks {
        if summary.boundary_is_breached || !geometry.is_valid() {
            continue;
        }
        let (Some(seed_cell), Some(mut wall_heights)) = (
            tank_habitat_seed_cell(region),
            height_queries.wall_height_images(habitat, limits),
        ) else {
            continue;
        };
        let Some(bounds) =
            height_queries.tank_height_bounds(habitat, region, limits, &wall_heights)
        else {
            continue;
        };
        height_queries.include_adjacent_tank_water_in_wall_minima(
            &mut wall_heights,
            habitat,
            limits,
            |entity| {
                let (_, _, geometry, adjacent, _, world) = tanks.get(entity).ok()?;
                (world.root == member.root && tank_habitat_seed_cell(adjacent) != Some(seed_cell))
                    .then_some(geometry.water_height)
            },
        );
        let rim = geometry.water_height + limits.water_offset_m;
        let kind = if target >= rim {
            TankEditKind::RaiseWall
        } else {
            TankEditKind::LowerWall
        };
        let mut edit = TankGeometryEdit {
            seed_cell,
            kind,
            definition: tank.definition,
            before: *geometry,
            after: *geometry,
            wall_heights,
        };
        step_tank_geometry_edit(&mut edit, target - rim, limits, bounds);
        if edit.before == edit.after || !edit.after.is_valid() {
            continue;
        }
        let preview = commands
            .spawn((
                edit,
                TankGeometryGesture {
                    kind,
                    pending: true,
                    bounds,
                },
                ConstructionPreview {
                    definition: tank.definition,
                    transform: Transform::default(),
                    validity: PlacementValidity::Valid { cost: Money::ZERO },
                },
                ScriptedTankGeometryEdit,
                *member,
            ))
            .id();
        commits.write(CommitConstruction { preview });
    }
}

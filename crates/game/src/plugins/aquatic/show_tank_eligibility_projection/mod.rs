use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::aquatic_simulation_types::{ShowTankEligible, Tank, TankGeometry};

pub(super) fn project_changed_tank_depth_into_show_eligibility(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    tanks: Query<
        (Entity, &TankGeometry, Has<ShowTankEligible>),
        (With<Tank>, Or<(Added<Tank>, Changed<TankGeometry>)>),
    >,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(minimum_depth) = definitions
        .tank_depth_policy()
        .map(|policy| policy.minimum_show_tank_depth_cm as f32 / 100.0)
    else {
        return;
    };
    for (tank_entity, geometry, currently_eligible) in &tanks {
        let should_be_eligible = geometry.is_valid() && geometry.depth() >= minimum_depth;
        if should_be_eligible && !currently_eligible {
            commands.entity(tank_entity).insert(ShowTankEligible);
        } else if !should_be_eligible && currently_eligible {
            commands.entity(tank_entity).remove::<ShowTankEligible>();
        }
    }
}

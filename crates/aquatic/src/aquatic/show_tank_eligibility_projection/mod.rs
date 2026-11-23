use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use super::aquatic_simulation_types::{ShowTankEligible, Tank, TankGeometry};

pub(super) fn reconcile_authored_show_tank_eligibility(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    tanks: Query<(Entity, Ref<Tank>, Ref<TankGeometry>, Has<ShowTankEligible>)>,
) {
    let catalogue_changed = definitions.is_changed() || assets.is_changed();
    let Some(minimum_depth) = definitions
        .get(&assets)
        .and_then(|definitions| definitions.tank_depth_policy())
        .map(|policy| policy.minimum_show_tank_depth_cm as f32 / 100.0)
    else {
        return;
    };
    for (entity, tank, geometry, is_eligible) in &tanks {
        if !catalogue_changed && !tank.is_added() && !geometry.is_changed() {
            continue;
        }
        let eligible = geometry.is_valid() && geometry.depth() >= minimum_depth;
        if eligible && !is_eligible {
            commands.entity(entity).insert(ShowTankEligible);
        } else if !eligible && is_eligible {
            commands.entity(entity).remove::<ShowTankEligible>();
        }
    }
}

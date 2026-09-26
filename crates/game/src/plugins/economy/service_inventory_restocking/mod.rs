use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::simulation_time::simulation_clock_types::ZooDayAdvanced;

use super::facility_economy_types::{Inventory, ServiceFacility};

pub(super) fn restock_service_facility_inventories_after_zoo_days_advance(
    mut advanced_days: MessageReader<ZooDayAdvanced>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut inventories: Query<(&ServiceFacility, &mut Inventory)>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for advanced_day in advanced_days.read() {
        let elapsed_days = advanced_day
            .current_day
            .saturating_sub(advanced_day.previous_day);
        if elapsed_days == 0 {
            continue;
        }
        for (facility, mut inventory) in &mut inventories {
            let Some(facility_definition) = world_definitions.find_facility(facility.definition)
            else {
                continue;
            };
            let restock_units = u32::from(facility_definition.inventory_restock_per_zoo_day)
                .saturating_mul(elapsed_days);
            inventory.restock(restock_units);
        }
    }
}

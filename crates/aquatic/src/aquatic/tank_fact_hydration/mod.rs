use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::aquatic_simulation_types::{Tank, TankCapacity, TankPopulation};

pub(super) fn hydrate_tank_capacity_and_population_facts(
    tanks: Query<(Entity, Has<TankCapacity>, Has<TankPopulation>), With<Tank>>,
    mut commands: Commands,
) {
    for (entity, has_capacity, has_population) in &tanks {
        let mut entity_commands = commands.entity(entity);
        if !has_capacity {
            entity_commands.insert(TankCapacity::default());
        }
        if !has_population {
            entity_commands.insert(TankPopulation::default());
        }
    }
}

pub(super) fn initialize_legacy_tank_water_limits(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut tanks: Query<
        (Entity, &mut super::aquatic_simulation_types::TankGeometry),
        With<super::aquatic_simulation_types::TankWaterLimitNeedsInitialization>,
    >,
) {
    let Some(surface) = active_definitions
        .get(&definitions)
        .and_then(|view| view.tank_surface_policy())
    else {
        return;
    };
    for (entity, mut geometry) in &mut tanks {
        if geometry.water_height
            < geometry.wall_height - surface.water_height_offset_cm as f32 / 100.0 - f32::EPSILON
        {
            geometry.water_height_limit = geometry.water_height;
        }
        commands
            .entity(entity)
            .remove::<super::aquatic_simulation_types::TankWaterLimitNeedsInitialization>();
    }
}

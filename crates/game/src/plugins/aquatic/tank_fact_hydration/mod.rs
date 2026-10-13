use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::aquatic_simulation_types::{
    Tank, TankCapacity, TankFiltration, TankGeometry, TankPopulation, TankSurfaceHydrated,
};

pub(super) fn hydrate_tank_capacity_population_and_filtration_facts(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut tanks: Query<
        (
            Entity,
            &Tank,
            Has<TankCapacity>,
            Has<TankPopulation>,
            Option<&mut TankFiltration>,
        ),
        With<Tank>,
    >,
    mut commands: Commands,
) {
    let definitions = active_definitions.get(&definitions);
    for (entity, tank, has_capacity, has_population, filtration) in &mut tanks {
        let Some(definition) = definitions.and_then(|view| view.find_tank(tank.definition)) else {
            // Filtration is derived from the current definition, not saved live state.
            // Missing definitions must not leave a previous tank's rate active.
            if filtration.is_some() {
                commands.entity(entity).remove::<TankFiltration>();
            }
            continue;
        };
        let mut entity_commands = commands.entity(entity);
        if !has_capacity {
            entity_commands.insert(TankCapacity::default());
        }
        if !has_population {
            entity_commands.insert(TankPopulation::default());
        }
        let authored_filtration = TankFiltration {
            litres_per_zoo_day: definition.filtration_per_day,
        };
        if let Some(mut filtration) = filtration {
            filtration.set_if_neq(authored_filtration);
        } else {
            entity_commands.insert(authored_filtration);
        }
    }
}

pub(super) fn hydrate_new_tank_surface_geometry_from_authored_offsets(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut tanks: Query<(Entity, &mut TankGeometry), (With<Tank>, Without<TankSurfaceHydrated>)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions.tank_surface_policy() else {
        return;
    };
    let water_offset = policy.water_height_offset_cm as f32 / 100.0;
    let floor_offset = policy.floor_height_offset_cm as f32 / 100.0;
    for (entity, mut geometry) in &mut tanks {
        let mut resolved_geometry = *geometry;
        resolved_geometry.water_height += water_offset;
        resolved_geometry.floor_height += floor_offset;
        resolved_geometry.volume = resolved_geometry.area * resolved_geometry.depth();
        if resolved_geometry.is_valid() {
            *geometry = resolved_geometry;
            commands.entity(entity).insert(TankSurfaceHydrated);
        }
    }
}

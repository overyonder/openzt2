use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::aquatic_simulation_types::{
    AddAquaticWaterDirt, AquaticHome, Tank, TankGeometry, WaterFilter, WaterQuality,
};

/// How far in front of a filter its tank's water is looked for.
const FILTER_TANK_REACH_METRES: f32 = 3.0;

/// Dirtiness is the share of a tank's volume that is dirt, so the same amount
/// matters less in a bigger tank. Water quality is its complement in permille.
fn dirt_as_water_quality_permille(amount: f32, volume: f32) -> u16 {
    if volume <= 0.0 || !amount.is_finite() {
        return 0;
    }
    (amount / volume * 1000.0).round().clamp(0.0, 1000.0) as u16
}

pub(super) fn hydrate_water_filters_from_object_definitions(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    objects: Query<(Entity, &DefinitionId), Added<DefinitionId>>,
    mut commands: Commands,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, definition) in &objects {
        let Some(policy) = catalogue
            .find_object(definition.0)
            .and_then(|object| object.water_filter)
        else {
            continue;
        };
        commands.entity(entity).insert(WaterFilter {
            policy,
            seconds_until_cleanup: policy.cleanup_interval_seconds,
            dirt_percent: 0.0,
        });
    }
}

pub(super) fn add_animal_dirt_to_home_tank_water(
    mut dirt: MessageReader<AddAquaticWaterDirt>,
    homes: Query<&AquaticHome>,
    mut tanks: Query<(&TankGeometry, &mut WaterQuality), With<Tank>>,
) {
    for dirt in dirt.read() {
        let Some((geometry, mut quality)) = homes
            .get(dirt.animal)
            .ok()
            .and_then(|home| tanks.get_mut(home.0).ok())
        else {
            continue;
        };
        quality.0 = quality
            .0
            .min(1000)
            .saturating_sub(dirt_as_water_quality_permille(dirt.amount, geometry.volume));
    }
}

/// At each cleanup a filter with room left moves dirt from the water it faces
/// into itself. A full filter does nothing until a worker cleans it.
pub(super) fn clean_tank_water_through_filters(
    time: Res<Time>,
    grid: Res<TopologyGrid>,
    habitats: Res<HabitatIndex>,
    mut filters: Query<(&GlobalTransform, &mut WaterFilter)>,
    mut tanks: Query<(&TankGeometry, &mut WaterQuality), With<Tank>>,
) {
    for (transform, mut filter) in &mut filters {
        filter.seconds_until_cleanup -= time.delta_secs();
        if filter.seconds_until_cleanup > 0.0 {
            continue;
        }
        filter.seconds_until_cleanup = filter.policy.cleanup_interval_seconds;
        if filter.dirt_percent >= 100.0 || filter.policy.maximum_dirt_capacity <= 0.0 {
            continue;
        }
        let origin = transform.translation().xz();
        let forward = transform.forward().xz().normalize_or_zero();
        // Placement doesn't turn filters to face their wall yet, so every side is searched.
        let tank = [forward, forward.perp(), -forward, -forward.perp()]
            .into_iter()
            .filter_map(|side| {
                locate_habitat_at_world_position(
                    &habitats,
                    origin + side * FILTER_TANK_REACH_METRES,
                    *grid,
                )
            })
            .find(|habitat| tanks.contains(*habitat));
        let Some((geometry, mut quality)) = tank.and_then(|tank| tanks.get_mut(tank).ok()) else {
            continue;
        };
        if quality.0 >= 1000 {
            continue;
        }
        quality.0 = quality
            .0
            .saturating_add(dirt_as_water_quality_permille(
                filter.policy.cleanup_rate,
                geometry.volume,
            ))
            .min(1000);
        filter.dirt_percent = (filter.dirt_percent
            + filter.policy.cleanup_rate * 100.0 / filter.policy.maximum_dirt_capacity)
            .min(100.0);
    }
}

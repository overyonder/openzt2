use std::time::Duration;

use bevy::prelude::*;

use openzt2_game_data::world_definitions::object_placement::ADJACENT_WATER_REACH_METRES;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::{Containment, HabitatMember};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};

use super::aquatic_simulation_types::{
    AddAquaticWaterDirt, AquaticAnimal, Tank, TankGeometry, WaterFilter, WaterQuality,
};

#[derive(Component)]
pub(super) struct FilterCleanupInterval(Timer);

#[derive(Component, PartialEq, Eq)]
#[relationship(relationship_target = TankWaterFilters)]
pub(super) struct FilterWaterTarget(Entity);

#[derive(Component)]
#[relationship_target(relationship = FilterWaterTarget)]
pub(super) struct TankWaterFilters(Vec<Entity>);

impl FilterCleanupInterval {
    fn from_seconds(seconds: f32) -> Self {
        Self(Timer::new(
            Duration::try_from_secs_f32(seconds).unwrap_or_default(),
            TimerMode::Once,
        ))
    }
}

/// Dirtiness is the share of a tank's volume that is dirt, so the same amount
/// matters less in a bigger tank. Water quality is its complement in permille.
fn dirt_as_water_quality_permille(amount: f32, volume: f32) -> u16 {
    if volume <= 0.0 || !amount.is_finite() {
        return 0;
    }
    (amount / volume * 1000.0).round().clamp(0.0, 1000.0) as u16
}

pub(super) fn reconcile_authored_water_filter_policy(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut objects: Query<(
        Entity,
        Ref<DefinitionId>,
        Option<&mut WaterFilter>,
        Option<&mut FilterCleanupInterval>,
    )>,
) {
    let catalogue_changed = definitions.is_changed() || assets.is_changed();
    let Some(catalogue) = definitions.get(&assets) else {
        return;
    };
    for (entity, definition, current, interval) in &mut objects {
        if !catalogue_changed
            && !definition.is_changed()
            && !(current.is_some() && interval.is_none())
        {
            continue;
        }
        let policy = catalogue
            .find_object(definition.0)
            .and_then(|object| object.water_filter);
        match (current, policy) {
            (Some(mut current), Some(policy)) => {
                if let Some(mut interval) = interval {
                    if current.policy.cleanup_interval_seconds != policy.cleanup_interval_seconds {
                        *interval =
                            FilterCleanupInterval::from_seconds(policy.cleanup_interval_seconds);
                    }
                } else {
                    commands
                        .entity(entity)
                        .insert(FilterCleanupInterval::from_seconds(
                            current.seconds_until_cleanup,
                        ));
                }
                if current.policy != policy {
                    current.policy = policy;
                }
            }
            (None, Some(policy)) => {
                commands.entity(entity).insert((
                    WaterFilter {
                        policy,
                        seconds_until_cleanup: policy.cleanup_interval_seconds,
                        dirt_percent: 0.0,
                    },
                    FilterCleanupInterval::from_seconds(policy.cleanup_interval_seconds),
                ));
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<WaterFilter>();
            }
            _ => {}
        }
    }
}

pub(super) fn remove_water_filter_timing_and_target(
    removed: On<Remove<WaterFilter>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<(FilterCleanupInterval, FilterWaterTarget)>();
}

pub(super) fn resolve_water_filter_targets(
    mut commands: Commands,
    roots: Query<(Ref<TopologyGrid>, Ref<HabitatIndex>)>,
    filters: Query<
        (
            Entity,
            Ref<GlobalTransform>,
            Ref<WorldMember>,
            Ref<FilterCleanupInterval>,
            Option<&FilterWaterTarget>,
        ),
        With<WaterFilter>,
    >,
    tanks: Query<(), With<Tank>>,
    promoted: Query<(), Added<Tank>>,
    mut demoted: RemovedComponents<Tank>,
) {
    let tanks_changed = !promoted.is_empty() | (demoted.read().count() != 0);
    for (entity, transform, member, interval, current) in &filters {
        let next = (|| {
            let (grid, habitats) = roots.get(member.root).ok()?;
            if !tanks_changed
                && !grid.is_changed()
                && !habitats.is_changed()
                && !transform.is_changed()
                && !member.is_changed()
                && !interval.is_added()
            {
                return current.map(|target| target.0);
            }
            let origin = transform.translation().xz();
            let forward = transform.forward().xz().normalize_or_zero();
            locate_habitat_at_world_position(
                &habitats,
                origin + forward * ADJACENT_WATER_REACH_METRES,
                *grid,
            )
            .filter(|tank| tanks.contains(*tank))
        })();
        if next != current.map(|target| target.0) {
            if let Some(tank) = next {
                commands.entity(entity).insert(FilterWaterTarget(tank));
            } else {
                commands.entity(entity).remove::<FilterWaterTarget>();
            }
        }
    }
}

pub(super) fn add_animal_dirt_to_home_tank_water(
    mut dirt: MessageReader<AddAquaticWaterDirt>,
    residents: Query<
        (&HabitatMember, &Containment),
        (With<AquaticAnimal>, With<Animal>, Without<Dead>),
    >,
    mut tanks: Query<(&TankGeometry, &mut WaterQuality), With<Tank>>,
) {
    for dirt in dirt.read() {
        let Some((geometry, mut quality)) = residents
            .get(dirt.animal)
            .ok()
            .filter(|(_, containment)| containment.is_contained)
            .and_then(|(member, _)| tanks.get_mut(member.habitat_entity).ok())
        else {
            continue;
        };
        let next = quality
            .0
            .min(1000)
            .saturating_sub(dirt_as_water_quality_permille(dirt.amount, geometry.volume));
        quality.set_if_neq(WaterQuality(next));
    }
}

pub(super) fn clean_tank_water_through_filters(
    time: Res<Time>,
    mut filters: Query<(
        &FilterWaterTarget,
        &mut WaterFilter,
        &mut FilterCleanupInterval,
    )>,
    mut tanks: Query<(&TankGeometry, &mut WaterQuality), With<Tank>>,
) {
    for (target, mut filter, mut interval) in &mut filters {
        let Ok((geometry, mut quality)) = tanks.get_mut(target.0) else {
            continue;
        };
        if !geometry.is_valid() || geometry.volume <= 0.0 {
            continue;
        }
        if !interval.0.tick(time.delta()).is_finished() {
            continue;
        }
        interval.0.set_duration(
            Duration::try_from_secs_f32(filter.policy.cleanup_interval_seconds).unwrap_or_default(),
        );
        interval.0.reset();
        if filter.dirt_percent >= 100.0 || filter.policy.maximum_dirt_capacity <= 0.0 {
            continue;
        }
        let removed_quality =
            dirt_as_water_quality_permille(filter.policy.cleanup_rate, geometry.volume)
                .min(1000_u16.saturating_sub(quality.0));
        if removed_quality == 0 {
            continue;
        }
        quality.0 += removed_quality;
        let removed = f32::from(removed_quality) * geometry.volume / 1000.0;
        filter.dirt_percent = (filter.dirt_percent
            + removed * 100.0 / filter.policy.maximum_dirt_capacity)
            .min(100.0);
    }
}

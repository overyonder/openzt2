use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_data::world_definitions::object_placement::ADJACENT_WATER_REACH_METRES;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::{Containment, HabitatMember, HabitatRegion};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::TerrainIndex,
    terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery},
};
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

/// The fractional permille below the published whole water-quality unit.
#[derive(Component, Default, PartialEq)]
pub(super) struct WaterQualityFraction(pub(super) f32);

#[derive(SystemParam)]
pub(super) struct FilterWaterSources<'w, 's> {
    tanks: Query<'w, 's, (&'static TankGeometry, &'static HabitatRegion), With<Tank>>,
    terrain: Res<'w, Assets<TerrainAsset>>,
    indexes: Query<'w, 's, Ref<'static, TerrainWaterMembershipIndex>>,
    regions: TerrainWaterRegionQuery<'w, 's>,
    roots: Query<'w, 's, &'static TerrainIndex>,
}

impl FilterWaterSources<'_, '_> {
    fn terrain_membership(&self, root: Entity) -> Option<Ref<'_, TerrainWaterMembershipIndex>> {
        let chunk = *self.roots.get(root).ok()?.chunks.get(&IVec2::new(0, -1))?;
        self.indexes.get(chunk).ok()
    }

    fn volume(&self, root: Entity, water: Entity) -> Option<f32> {
        let volume = if let Ok((geometry, habitat)) = self.tanks.get(water) {
            geometry.is_valid().then_some(geometry.depth())?
                * habitat.topology_cells.len() as f32
                * 0.75
        } else {
            let source = self.regions.get(water)?;
            let terrain = self.terrain.get(source.terrain())?;
            let depth = source.biome(terrain)?.deep_water_depth_mm? as f32 * 0.001;
            let membership = self.terrain_membership(root)?;
            if membership.terrain() != source.terrain().id() {
                return None;
            }
            depth * membership.accepted_points(water).count() as f32 * 0.75
        };
        (volume.is_finite() && volume > 0.0).then_some(volume)
    }
}

impl FilterCleanupInterval {
    fn from_seconds(seconds: f32) -> Self {
        Self(Timer::new(
            Duration::try_from_secs_f32(seconds).unwrap_or_default(),
            TimerMode::Once,
        ))
    }
}

/// Returns the accepted change in dirt after clamping to the water's capacity.
fn apply_water_dirt(
    mut quality: Mut<WaterQuality>,
    mut fraction: Mut<WaterQualityFraction>,
    volume: f32,
    amount: f32,
) -> f32 {
    if !amount.is_finite() {
        return 0.0;
    }
    let units = (f32::from(quality.0) + fraction.0).min(1000.0);
    let next = (units - amount / volume * 1000.0).clamp(0.0, 1000.0);
    let whole = next as u16;
    quality.set_if_neq(WaterQuality(whole));
    fraction.set_if_neq(WaterQualityFraction(next - f32::from(whole)));
    (units - next) * volume * 0.001
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
                            policy.cleanup_interval_seconds,
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
    water: FilterWaterSources,
    promoted: Query<(), Added<Tank>>,
    mut demoted: RemovedComponents<Tank>,
) {
    let tanks_changed = !promoted.is_empty() | (demoted.read().count() != 0);
    for (entity, transform, member, interval, current) in &filters {
        let next = (|| {
            let (grid, habitats) = roots.get(member.root).ok()?;
            let terrain = water.terrain_membership(member.root);
            if !tanks_changed
                && !grid.is_changed()
                && !habitats.is_changed()
                && !transform.is_changed()
                && !member.is_changed()
                && !interval.is_added()
                && !terrain.as_ref().is_some_and(DetectChanges::is_changed)
                && current.is_none_or(|target| {
                    tanks.contains(target.0) || water.regions.get(target.0).is_some()
                })
            {
                return current.map(|target| target.0);
            }
            let origin = transform.translation().xz();
            let forward = transform.forward().xz().normalize_or_zero();
            let probe = origin + forward * ADJACENT_WATER_REACH_METRES;
            locate_habitat_at_world_position(&habitats, probe, *grid)
                .filter(|tank| tanks.contains(*tank))
                .or_else(|| {
                    let membership = terrain?;
                    let asset = water.terrain.get(membership.terrain())?;
                    membership
                        .owner_at_world_xz(probe, asset)?
                        .filter(|&owner| water.regions.get(owner).is_some())
                })
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
        (&HabitatMember, &Containment, &WorldMember),
        (With<AquaticAnimal>, With<Animal>, Without<Dead>),
    >,
    water: FilterWaterSources,
    mut tanks: Query<(&mut WaterQuality, &mut WaterQualityFraction), With<Tank>>,
) {
    for dirt in dirt.read() {
        let Some((member, _, world)) = residents
            .get(dirt.animal)
            .ok()
            .filter(|(_, containment, _)| containment.is_contained)
        else {
            continue;
        };
        let (Some(volume), Ok((quality, fraction))) = (
            water.volume(world.root, member.habitat_entity),
            tanks.get_mut(member.habitat_entity),
        ) else {
            continue;
        };
        apply_water_dirt(quality, fraction, volume, dirt.amount);
    }
}

pub(super) fn clean_tank_water_through_filters(
    time: Res<Time<Fixed>>,
    real_time: Res<Time<Real>>,
    localization: Res<LocalizationPrecedenceIndex>,
    localized_assets: Res<Assets<LocalizationAsset>>,
    mut messages: Query<&mut ZooMessages>,
    mut filters: Query<(
        &FilterWaterTarget,
        &WorldMember,
        &mut WaterFilter,
        &mut FilterCleanupInterval,
    )>,
    water: FilterWaterSources,
    mut quality: Query<(&mut WaterQuality, &mut WaterQualityFraction)>,
) {
    for (target, owner, mut filter, mut interval) in &mut filters {
        let (Some(volume), Ok((quality, fraction))) = (
            water.volume(owner.root, target.0),
            quality.get_mut(target.0),
        ) else {
            continue;
        };
        if !interval.0.tick(time.timestep()).is_finished() {
            continue;
        }
        interval.0.set_duration(
            Duration::try_from_secs_f32(filter.policy.cleanup_interval_seconds).unwrap_or_default(),
        );
        interval.0.reset();
        if quality.0 >= 1000 {
            continue;
        }
        if filter.dirt_percent >= 100.0 {
            let text = filter
                .policy
                .full_filter_message
                .and_then(|key| {
                    localization
                        .borrow_loaded_localization_view(&localized_assets)?
                        .find_plain_localized_text(key)
                })
                .filter(|text| !text.is_empty());
            if let (Some(text), Ok(mut messages)) = (text, messages.get_mut(owner.root)) {
                messages.messages.push(ZooMessage {
                    text: text.to_owned(),
                    expires_at_seconds: real_time.elapsed_secs_f64() + 60.0,
                });
            }
            continue;
        }
        if filter.policy.maximum_dirt_capacity <= 0.0 {
            continue;
        }
        let removed = -apply_water_dirt(
            quality,
            fraction,
            volume,
            -filter.policy.cleanup_rate.max(0.0),
        );
        if removed <= 0.0 {
            continue;
        }
        filter.dirt_percent = (filter.dirt_percent
            + removed * 100.0 / filter.policy.maximum_dirt_capacity)
            .min(100.0);
    }
}

use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashMap, prelude::*};
use openzt2_game_data::species::{NeedKind, SpeciesHabitatSpace, SpeciesNeed};
use openzt2_game_data::AssetId;

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::AquaticAnimal;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::habitat::habitat_types::HabitatMembers;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use super::calculations::clamp_animal_need_q16_after_delta;
use super::need_updates::{observe_need_write, NeedLatch, NeedThresholdCrossing};
use super::types::{EnvironmentHabitatAssessed, EnvironmentNeed, HabitatSuitability, Q16_ONE};

#[derive(Component)]
pub(super) struct HabitatSuitabilityDirty;

#[derive(Component, Default, PartialEq)]
pub(super) struct HabitatSpeciesSuitability(HashMap<AssetId, (u32, HabitatSuitability)>);

pub(super) fn mark_habitat_after_occupant_eligibility_changes(
    change: On<
        Insert<(
            Animal,
            SpeciesHandle,
            EnvironmentNeed,
            Dead,
            WorldEntityIsCrated,
            AquaticAnimal,
        )>,
    >,
    membership: Query<&HabitatMember>,
    mut commands: Commands,
) {
    if let Ok(member) = membership.get(change.entity) {
        commands
            .entity(member.habitat_entity)
            .try_insert(HabitatSuitabilityDirty);
    }
}

pub(super) fn mark_habitat_after_occupant_eligibility_is_removed(
    change: On<
        Remove<(
            Animal,
            SpeciesHandle,
            Dead,
            WorldEntityIsCrated,
            AquaticAnimal,
        )>,
    >,
    membership: Query<&HabitatMember>,
    mut commands: Commands,
) {
    if let Ok(member) = membership.get(change.entity) {
        commands
            .entity(member.habitat_entity)
            .try_insert(HabitatSuitabilityDirty);
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn project_changed_habitat_suitability(
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    dirty_habitats: Query<
        Entity,
        Or<(
            Changed<HabitatMembers>,
            Changed<HabitatSummary>,
            Changed<HabitatRegion>,
            With<HabitatSuitabilityDirty>,
        )>,
    >,
    mut habitats: Query<(
        Entity,
        Ref<HabitatSummary>,
        Ref<HabitatRegion>,
        Option<&HabitatMembers>,
        Option<&mut HabitatSpeciesSuitability>,
        Has<HabitatSuitabilityDirty>,
    )>,
    occupants: Query<
        &SpeciesHandle,
        (
            With<Animal>,
            Without<Dead>,
            Without<WorldEntityIsCrated>,
            Without<AquaticAnimal>,
        ),
    >,
    mut needs: Query<(
        &mut HabitatSuitability,
        &mut EnvironmentNeed,
        &mut NeedLatch<7>,
        Has<EnvironmentHabitatAssessed>,
    )>,
    mut commands: Commands,
    mut changed: MessageWriter<NeedThresholdCrossing<7>>,
) {
    let Some(species_view) = index.get(&assets) else {
        return;
    };
    let all_changed = index.is_changed() || assets.is_changed();
    let affected: Vec<Entity> = if all_changed {
        habitats.iter().map(|(entity, ..)| entity).collect()
    } else {
        dirty_habitats.iter().collect()
    };
    for habitat in affected {
        let Ok((_, summary, region, members, suitability, dirty)) = habitats.get_mut(habitat)
        else {
            continue;
        };
        if dirty {
            commands.entity(habitat).remove::<HabitatSuitabilityDirty>();
        }
        let area = summary.land_area_square_metres + summary.water_area_square_metres;
        if !area.is_finite()
            || area <= 0.0
            || (area - region.area_square_metres).abs() > area * f32::EPSILON * 4.0
        {
            continue;
        }
        let mut populations: HashMap<AssetId, Vec<Entity>> = HashMap::default();
        for animal in members.into_iter().flat_map(RelationshipTarget::iter) {
            if let Ok(species) = occupants.get(animal) {
                populations.entry(species.species).or_default().push(animal);
            }
        }
        let mut projected = HabitatSpeciesSuitability::default();
        let geometry_changed = all_changed || summary.is_changed() || region.is_changed();
        for (species, members) in populations {
            let Some(definition) = species_view.find(species) else {
                continue;
            };
            let (Some(requirement), Some(row)) = (
                definition.habitat_space,
                definition
                    .needs
                    .iter()
                    .find(|row| row.kind == NeedKind::Environment),
            ) else {
                continue;
            };
            let population = u32::try_from(members.len()).unwrap_or(u32::MAX);
            let cached = suitability
                .as_deref()
                .and_then(|suitability| suitability.0.get(&species));
            let next = match cached {
                Some((previous_population, next))
                    if !geometry_changed && *previous_population == population =>
                {
                    *next
                }
                _ => calculate_species_habitat_suitability(&summary, requirement, row, population),
            };
            projected.0.insert(species, (population, next));
            for animal in members {
                let Ok((mut current, environment, latch, assessed)) = needs.get_mut(animal) else {
                    continue;
                };
                if assessed && *current == next {
                    continue;
                }
                if !assessed {
                    commands.entity(animal).insert(EnvironmentHabitatAssessed);
                }
                let old = environment.0;
                let value = update_environment_habitat_baseline(
                    old,
                    assessed.then_some(current.overall),
                    next.overall,
                );
                current.set_if_neq(next);
                environment
                    .map_unchanged(|need| &mut need.0)
                    .set_if_neq(value);
                observe_need_write(
                    animal,
                    NeedKind::Environment,
                    old,
                    value,
                    row.trigger_wellness_threshold,
                    row.cessation_wellness_threshold,
                    latch,
                    &mut changed,
                );
            }
        }
        if let Some(mut current) = suitability {
            current.set_if_neq(projected);
        } else if !projected.0.is_empty() {
            commands.entity(habitat).insert(projected);
        }
    }
}

fn update_environment_habitat_baseline(
    current_q16: i32,
    previous_baseline: Option<u16>,
    next_baseline: u16,
) -> i32 {
    match previous_baseline {
        Some(previous) => clamp_animal_need_q16_after_delta(
            current_q16,
            (i32::from(next_baseline) - i32::from(previous)) * Q16_ONE,
        ),
        None => i32::from(next_baseline) * Q16_ONE,
    }
}

fn calculate_species_habitat_suitability(
    summary: &HabitatSummary,
    requirement: SpeciesHabitatSpace,
    environment: &SpeciesNeed,
    population: u32,
) -> HabitatSuitability {
    let land = f64::from(summary.land_area_square_metres);
    let water = f64::from(summary.water_area_square_metres);
    let initial = f64::from(f32::from(requirement.initial_square_metres));
    let additional = f64::from(f32::from(requirement.additional_square_metres));
    if population == 0
        || summary.boundary_is_breached
        || [land, water]
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
    {
        return HabitatSuitability::default();
    }
    // This measures region area sufficiency for the species group. It does not
    // establish which cells an individual animal can traverse.
    let required = initial + additional * f64::from(population.saturating_sub(1));
    let space = if required == 0.0 {
        1000
    } else {
        ((land + water) / required * 1000.0).clamp(0.0, 1000.0) as u16
    };
    let biome = resolve_biome_map_score(summary, environment);
    HabitatSuitability {
        space,
        biome,
        overall: space.min(biome),
    }
}

#[cfg(test)]
mod tests;

/// Engine policy: linearly map authored -10..10 sensitivities to wellness,
/// then average over occupied terrain area. Space caps this biome wellness.
fn resolve_biome_map_score(summary: &HabitatSummary, environment: &SpeciesNeed) -> u16 {
    let total = summary.land_area_square_metres + summary.water_area_square_metres;
    if total <= 0.0 || !total.is_finite() {
        return 0;
    }
    let preferences = &environment.preferences;
    if preferences.is_empty() {
        return 1000;
    }
    // Unspecified terrain has neutral sensitivity; each terrain cell contributes
    // once through the habitat summary, independent of preference count.
    let weighted = summary.biome_areas_square_metres.iter().fold(
        f64::from(total) * 500.0,
        |quality, (biome, area)| {
            let sensitivity = preferences
                .iter()
                .find(|row| row.target == *biome)
                .map_or(0, |row| row.weight);
            quality + f64::from(*area) * f64::from(sensitivity) * 50.0
        },
    );
    (weighted / f64::from(total)).round().clamp(0.0, 1000.0) as u16
}

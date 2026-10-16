use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::species::{NeedKind, SpeciesHabitatSpace, SpeciesNeed};

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::AquaticAnimal;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use super::calculations::{
    animal_need_crossed_authored_threshold, clamp_animal_need_q16_after_delta,
    convert_animal_need_q16_to_permille,
};
use super::types::{
    EnvironmentHabitatAssessed, EnvironmentNeed, HabitatSuitability, NeedChanged, NeedTriggerState,
    Q16_ONE,
};

// Both queries read membership; only the assessment query writes suitability.
#[allow(clippy::type_complexity)]
pub(super) fn assess_habitat_suitability(
    mut commands: Commands,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    occupants: Query<
        (&SpeciesHandle, &HabitatMember),
        (
            With<Animal>,
            Without<Dead>,
            Without<WorldEntityIsCrated>,
            Without<AquaticAnimal>,
        ),
    >,
    mut animals: Query<
        (
            Entity,
            &SpeciesHandle,
            Option<&HabitatMember>,
            Has<WorldEntityIsCrated>,
            &mut HabitatSuitability,
            &mut EnvironmentNeed,
            &mut NeedTriggerState,
            Has<EnvironmentHabitatAssessed>,
        ),
        (With<Animal>, Without<Dead>, Without<AquaticAnimal>),
    >,
    habitats: Query<(&HabitatSummary, &HabitatRegion)>,
    mut changed: MessageWriter<NeedChanged>,
) {
    // Rebuild from live membership so transfers, deaths, crating and reloads do
    // not depend on a terrain/fence change message or a second population owner.
    let mut populations = HashMap::<_, u32>::default();
    for (species, member) in &occupants {
        let population = populations
            .entry((member.habitat_entity, species.species))
            .or_default();
        *population = population.saturating_add(1);
    }
    let species_view = index.get(&assets);
    for (entity, species, member, crated, mut current, mut environment, mut triggers, assessed) in
        &mut animals
    {
        if crated {
            continue;
        }
        let Some(member) = member else {
            continue;
        };
        let (Ok((summary, region)), Some(population), Some(definition)) = (
            habitats.get(member.habitat_entity),
            populations.get(&(member.habitat_entity, species.species)),
            species_view.and_then(|view| view.find(species.species)),
        ) else {
            continue;
        };
        let Some(row) = definition
            .needs
            .iter()
            .find(|row| row.kind == NeedKind::Environment)
        else {
            continue;
        };
        // A missing terrain summary or definition is not an unsuitable habitat.
        let Some(requirement) = definition.habitat_space else {
            continue;
        };
        let area = summary.land_area_square_metres + summary.water_area_square_metres;
        if !area.is_finite()
            || area <= 0.0
            || (area - region.area_square_metres).abs() > area * f32::EPSILON * 4.0
        {
            continue;
        }
        let next = calculate_species_habitat_suitability(summary, requirement, row, *population);
        if *current != next || !assessed {
            if !assessed {
                commands.entity(entity).insert(EnvironmentHabitatAssessed);
            }
            let old = environment.0;
            let value = update_environment_habitat_baseline(
                old,
                assessed.then_some(current.overall),
                next.overall,
            );
            *current = next;
            environment.0 = value;
            triggers.observe(
                7,
                value,
                row.trigger_wellness_threshold,
                row.cessation_wellness_threshold,
            );
            if animal_need_crossed_authored_threshold(
                old,
                value,
                row.trigger_wellness_threshold,
                row.cessation_wellness_threshold,
            ) {
                changed.write(NeedChanged {
                    animal: entity,
                    need: NeedKind::Environment,
                    value: convert_animal_need_q16_to_permille(value),
                });
            }
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

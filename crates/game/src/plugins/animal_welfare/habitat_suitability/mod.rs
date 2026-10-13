use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::species::{NeedKind, Species, SpeciesNeed};

use crate::plugins::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    aquatic::aquatic_simulation_types::AquaticAnimal,
    habitat::habitat_types::{HabitatMember, HabitatSummary},
    world_spawn::world_entity_crating::WorldEntityIsCrated,
};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};

use super::types::HabitatSuitability;

// Both queries read membership; only the assessment query writes suitability.
#[allow(clippy::type_complexity)]
pub(super) fn assess_habitat_suitability(
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
            &SpeciesHandle,
            Option<&HabitatMember>,
            Has<WorldEntityIsCrated>,
            &mut HabitatSuitability,
        ),
        (With<Animal>, Without<Dead>, Without<AquaticAnimal>),
    >,
    habitats: Query<&HabitatSummary>,
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
    for (species, member, crated, mut current) in &mut animals {
        let next = (!crated)
            .then_some(member)
            .flatten()
            .and_then(|member| {
                let summary = habitats.get(member.habitat_entity).ok()?;
                let population = populations.get(&(member.habitat_entity, species.species))?;
                let definition = species_view?.find(species.species)?;
                Some(calculate_species_habitat_suitability(
                    summary,
                    definition,
                    *population,
                ))
            })
            .unwrap_or_default();
        if *current != next {
            *current = next;
        }
    }
}

fn calculate_species_habitat_suitability(
    summary: &HabitatSummary,
    species: &Species,
    population: u32,
) -> HabitatSuitability {
    let Some(requirement) = species.habitat_space else {
        return HabitatSuitability::default();
    };
    let Some(environment) = species
        .needs
        .iter()
        .find(|need| need.kind == NeedKind::Environment)
    else {
        return HabitatSuitability::default();
    };
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

/// Weight the habitat biome areas by the species preferences.
fn resolve_biome_map_score(summary: &HabitatSummary, environment: &SpeciesNeed) -> u16 {
    let total = summary.land_area_square_metres + summary.water_area_square_metres;
    if total <= 0.0 || !total.is_finite() {
        return 0;
    }
    let preferences = &environment.preferences;
    if preferences.is_empty() {
        return 1000;
    }
    let mut weighted = 0.0_f64;
    let mut weights = 0.0_f64;
    for preference in preferences {
        let weight = f64::from(preference.weight.max(0));
        let area = summary
            .biome_areas_square_metres
            .iter()
            .find(|(id, _)| id.0 == preference.target.0)
            .map_or(0.0, |(_, area)| *area);
        let quality = ((area / total).clamp(0.0, 1.0) * 1000.0).round() as u16;
        let accepted = if quality >= preference.minimum_quality {
            quality
        } else {
            0
        };
        weighted += f64::from(accepted) * weight;
        weights += weight;
    }
    if weights == 0.0 {
        1000
    } else {
        (weighted / weights).round().clamp(0.0, 1000.0) as u16
    }
}

use super::*;
use openzt2_game_data::{
    species::{NeedPreference, NonNegativeSquareMetres},
    AssetId,
};

#[test]
fn weather_penalty_survives_changes_to_habitat_baseline() {
    let initial = update_environment_habitat_baseline(750 * Q16_ONE, None, 800);
    let weather_adjusted = clamp_animal_need_q16_after_delta(initial, -25 * Q16_ONE);
    let terrain_changed = update_environment_habitat_baseline(weather_adjusted, Some(800), 790);
    assert_eq!(terrain_changed, 765 * Q16_ONE);
    let terrain_restored = update_environment_habitat_baseline(terrain_changed, Some(790), 800);
    assert_eq!(terrain_restored, weather_adjusted);
}

#[test]
fn terrain_and_population_changes_recalculate_environment_quality() {
    let preferred = AssetId::from_key("tundra");
    let neutral = AssetId::from_key("grassland");
    let hostile = AssetId::from_key("desert");
    let need = SpeciesNeed {
        kind: NeedKind::Environment,
        initial_wellness_permille_range: [750; 2],
        wellness_adjustment_per_update_q16: 0,
        cessation_wellness_threshold: None,
        trigger_wellness_threshold: Some(500),
        critical_wellness_threshold: Some(100),
        display_wellness_threshold: None,
        pressing_wellness_threshold: None,
        advanced: false,
        preferences: [(preferred, 10), (neutral, 0), (hostile, -10)]
            .map(|(target, weight)| NeedPreference {
                kind: NeedKind::Environment,
                target,
                weight,
                minimum_quality: 0,
            })
            .into(),
    };
    let requirement = SpeciesHabitatSpace {
        initial_square_metres: NonNegativeSquareMetres::from_finite_non_negative(320.0).unwrap(),
        additional_square_metres: NonNegativeSquareMetres::from_finite_non_negative(80.0).unwrap(),
    };
    let mut summary = HabitatSummary {
        land_area_square_metres: 320.0,
        water_area_square_metres: 0.0,
        biome_areas_square_metres: Box::new([(neutral, 320.0)]),
        boundary_is_breached: false,
    };
    let assess = |summary: &HabitatSummary, population| {
        calculate_species_habitat_suitability(summary, requirement, &need, population)
    };
    assert_eq!(assess(&summary, 1).overall, 500);
    summary.biome_areas_square_metres = Box::new([(hostile, 320.0)]);
    assert_eq!(assess(&summary, 1).overall, 0);
    summary.biome_areas_square_metres = Box::new([(preferred, 320.0)]);
    assert_eq!(assess(&summary, 1).overall, 1000);
    assert_eq!(assess(&summary, 2).overall, 800);
    summary.biome_areas_square_metres = Box::new([(preferred, 160.0), (hostile, 160.0)]);
    assert_eq!(assess(&summary, 1).overall, 500);
    summary.boundary_is_breached = true;
    assert_eq!(assess(&summary, 1).overall, 0);
}

use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::guests::guest_survey_types::{GuestSurvey, GuestSurveyRating};

use super::{
    award_and_progression_fact_types::{
        EarnedProgressionAward, ProgressionFactChanged, ScenarioAwardPointTotal,
    },
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::{
        catalogue_definition_is_unlocked_at_index, mark_catalogue_definition_unlocked_at_index,
    },
    fame_calculations::{
        calculate_authentic_zoo_fame, select_fame_level_for_rating_and_guest_count,
        update_current_and_maximum_fame, ZooFameCalculationInputs,
    },
    fame_types::Fame,
    profile_challenge_types::TotalAnimalsReleasedCount,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
    zoo_rating_input_types::{GuestRatingFacts, LivingAnimalPopulation},
};

pub(super) fn apply_scenario_maximum_fame_requests(
    mut requests: MessageReader<openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
    mut fame: ResMut<Fame>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    for request in requests.read() {
        if !request.percent.is_finite() || !(0.0..=100.0).contains(&request.percent) {
            continue;
        }
        if fame
            .maximum_percent_reached
            .is_none_or(|previous| request.percent > previous)
        {
            fame.maximum_percent_reached = Some(request.percent);
            fame.maximum_reached = fame
                .maximum_reached
                .max(Fame::half_stars_for_percent(request.percent));
            changed.write(ProgressionFactChanged::Fame);
        }
    }
}

pub(super) fn update_fame_and_apply_authored_fame_unlocks(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    animals: Res<LivingAnimalPopulation>,
    guests: Res<GuestRatingFacts>,
    survey: Option<Res<GuestSurvey>>,
    scenario_award_points: Res<ScenarioAwardPointTotal>,
    earned_awards: Query<&EarnedProgressionAward>,
    added_awards: Query<(), Added<EarnedProgressionAward>>,
    mut removed_awards: RemovedComponents<EarnedProgressionAward>,
    total_animals_released: Res<TotalAnimalsReleasedCount>,
    mut fame: ResMut<Fame>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let awards_removed = removed_awards.read().count() != 0;
    if !definitions.is_changed()
        && !active_definitions.is_changed()
        && !rating.is_changed()
        && !animals.is_changed()
        && !guests.is_changed()
        && !scenario_award_points.is_changed()
        && !total_animals_released.is_changed()
        && !survey.as_ref().is_some_and(|survey| survey.is_changed())
        && added_awards.is_empty()
        && !awards_removed
    {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let guest_count = guests.count;

    let (next_half_stars, current_percent) = if definitions.fame_thresholds().next().is_some() {
        if !rating.overall_available {
            return;
        }
        let next = select_fame_level_for_rating_and_guest_count(
            definitions.fame_thresholds().map(|threshold| {
                (
                    threshold.level,
                    threshold.minimum_rating,
                    threshold.minimum_guests,
                )
            }),
            rating.overall_permille,
            guest_count,
        );
        (next, f32::from(next) * 10.0)
    } else {
        let award_and_scenario_points = scenario_award_points
            .0
            .saturating_add(u32::try_from(earned_awards.iter().len()).unwrap_or(u32::MAX));
        let education_view_rate = survey.as_ref().map_or(rating.education_permille, |s| {
            s.rating_permille(GuestSurveyRating::Education)
                .unwrap_or(rating.education_permille)
                .max(rating.education_permille)
        });
        let entertainment_view_rate =
            survey
                .as_ref()
                .map_or(rating.guest_satisfaction_permille, |s| {
                    s.rating_permille(GuestSurveyRating::Entertainment)
                        .unwrap_or(rating.guest_satisfaction_permille)
                        .max(rating.guest_satisfaction_permille)
                });
        calculate_authentic_zoo_fame(&ZooFameCalculationInputs {
            animal_count: animals.animals,
            species_count: animals.species,
            animal_happiness_permille: rating.animal_welfare_permille,
            guest_count,
            guest_happiness_permille: rating.guest_satisfaction_permille,
            education_view_rate_permille: education_view_rate,
            entertainment_view_rate_permille: entertainment_view_rate,
            award_and_scenario_points,
            animals_released: total_animals_released.0,
        })
    };

    let mut next_fame = *fame;
    if update_current_and_maximum_fame(&mut next_fame, next_half_stars, current_percent) {
        *fame = next_fame;
        changed.write(ProgressionFactChanged::Fame);
    }
    for threshold in definitions
        .fame_thresholds()
        .filter(|threshold| threshold.level <= u16::from(next_half_stars))
    {
        let targets = &threshold.unlocks;
        if !targets
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        for target in targets {
            let id = *target;
            if let Some(index) = find_catalogue_definition_index(definitions, id) {
                if catalogue_definition_is_unlocked_at_index(&unlocks, index) {
                    continue;
                }
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(id));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progression::zoo_rating_recomputation::{
        recompute_guest_ratings, reduce_living_animal_population,
    };
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
    use openzt2_game_data::AssetId;
    use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};
    use openzt2_zoo_state::guests::guest_simulation_types::Guest;

    #[test]
    fn authentic_fame_progression_advances_fame_and_emits_change() {
        let mut app = App::new();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .init_resource::<WorldDefinitions>()
            .init_resource::<ZooRating>()
            .init_resource::<Fame>()
            .init_resource::<UnlockedCatalogueDefinitionSet>()
            .init_resource::<ScenarioAwardPointTotal>()
            .init_resource::<TotalAnimalsReleasedCount>()
            .add_message::<ProgressionFactChanged>()
            .init_resource::<LivingAnimalPopulation>()
            .init_resource::<GuestRatingFacts>()
            .add_systems(
                Update,
                (
                    (reduce_living_animal_population, recompute_guest_ratings),
                    update_fame_and_apply_authored_fame_unlocks,
                )
                    .chain(),
            );

        let world_document = WorldDefinitionDocument::default();
        let world_handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(
                world_document.clone(),
            ));
        app.world_mut()
            .resource_mut::<WorldDefinitions>()
            .index_test_document(world_handle, &world_document);

        app.world_mut().resource_mut::<Fame>().half_stars = 1;
        app.world_mut()
            .resource_mut::<ZooRating>()
            .animal_welfare_permille = 700;
        app.world_mut()
            .resource_mut::<ZooRating>()
            .guest_satisfaction_permille = 800;
        app.world_mut()
            .resource_mut::<ZooRating>()
            .education_permille = 400;
        app.world_mut().resource_mut::<ScenarioAwardPointTotal>().0 = 1;

        let species_a = AssetId::from_key("species-a");
        let species_b = AssetId::from_key("species-b");

        app.world_mut()
            .spawn((Animal, SpeciesHandle { species: species_a }));
        app.world_mut()
            .spawn((Animal, SpeciesHandle { species: species_a }));
        app.world_mut()
            .spawn((Animal, SpeciesHandle { species: species_b }));
        app.world_mut()
            .spawn((Animal, SpeciesHandle { species: species_b }));

        for _ in 0..10 {
            app.world_mut().spawn(Guest);
        }

        app.update();

        let fame = *app.world().resource::<Fame>();
        assert_eq!(fame.half_stars, 2);
        assert_eq!(fame.maximum_reached, 2);
        assert!(fame.maximum_percent_reached.is_some_and(|p| p >= 20.0));

        let events = app.world().resource::<Messages<ProgressionFactChanged>>();
        assert!(!events.is_empty());
    }
}

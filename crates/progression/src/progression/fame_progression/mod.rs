use std::collections::BTreeSet;

use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};
use openzt2_zoo_state::guests::guest_simulation_types::{Guest, GuestSurvey};

use super::{
    award_and_progression_fact_types::{
        EarnedProgressionAward, ProgressionFactChanged, ScenarioAwardPointTotal,
    },
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    fame_calculations::{
        calculate_authentic_zoo_fame, select_fame_level_for_rating_and_guest_count,
        update_current_and_maximum_fame, ZooFameCalculationInputs,
    },
    fame_types::Fame,
    profile_challenge_types::TotalAnimalsReleasedCount,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
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
                .max((request.percent / 10.0).floor() as u8);
            changed.write(ProgressionFactChanged::Fame);
        }
    }
}

pub(super) fn update_fame_and_apply_authored_fame_unlocks(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    animals: Query<&SpeciesHandle, (With<Animal>, Without<Dead>)>,
    guests: Query<(), With<Guest>>,
    survey: Option<Res<GuestSurvey>>,
    scenario_award_points: Res<ScenarioAwardPointTotal>,
    earned_awards: Query<&EarnedProgressionAward>,
    total_animals_released: Res<TotalAnimalsReleasedCount>,
    mut fame: ResMut<Fame>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let guest_count = guests.iter().count() as u32;

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
        let mut unique_species = BTreeSet::new();
        let mut animal_count = 0_u32;
        for handle in &animals {
            animal_count = animal_count.saturating_add(1);
            unique_species.insert(handle.species);
        }
        let award_and_scenario_points = scenario_award_points
            .0
            .saturating_add(earned_awards.iter().count() as u32);
        let education_view_rate = survey.as_ref().map_or(rating.education_permille, |s| {
            s.education_view_rate_permille.max(rating.education_permille)
        });
        let entertainment_view_rate =
            survey
                .as_ref()
                .map_or(rating.guest_satisfaction_permille, |s| {
                    s.entertainment_view_rate_permille
                        .max(rating.guest_satisfaction_permille)
                });
        calculate_authentic_zoo_fame(&ZooFameCalculationInputs {
            animal_count,
            species_count: unique_species.len() as u32,
            animal_happiness_permille: rating.animal_welfare_permille,
            guest_count,
            guest_happiness_permille: rating.guest_satisfaction_permille,
            education_view_rate_permille: education_view_rate,
            entertainment_view_rate_permille: entertainment_view_rate,
            award_and_scenario_points,
            animals_released: total_animals_released.0,
        })
    };

    if update_current_and_maximum_fame(&mut fame, next_half_stars, current_percent) {
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
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
    use openzt2_game_data::AssetId;

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
            .add_systems(Update, update_fame_and_apply_authored_fame_unlocks);

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
        app.world_mut().resource_mut::<ZooRating>().animal_welfare_permille = 700;
        app.world_mut().resource_mut::<ZooRating>().guest_satisfaction_permille = 800;
        app.world_mut().resource_mut::<ZooRating>().education_permille = 400;
        app.world_mut().resource_mut::<ScenarioAwardPointTotal>().0 = 1;

        let species_a = AssetId::from_key("species-a");
        let species_b = AssetId::from_key("species-b");

        app.world_mut().spawn((Animal, SpeciesHandle { species: species_a }));
        app.world_mut().spawn((Animal, SpeciesHandle { species: species_a }));
        app.world_mut().spawn((Animal, SpeciesHandle { species: species_b }));
        app.world_mut().spawn((Animal, SpeciesHandle { species: species_b }));

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

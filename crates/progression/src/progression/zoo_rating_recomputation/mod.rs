use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::RatingInputKind;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};
use openzt2_zoo_state::animal_welfare::types::{AnimalWelfare, NeedPoints};
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::guests::{
    guest_simulation_types::{Guest, GuestEducation, GuestSatisfaction, GuestSpeciesViewCounts},
    guest_survey_types::GuestSurveyExperience,
};
use openzt2_zoo_state::progression::fame_types::GuestSpeciesFameViews;
use openzt2_zoo_state::maintenance::maintenance_types::ZooCleanlinessPermille;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    rating_types::ZooRating,
    zoo_rating_calculations::{
        calculate_mean_permille, calculate_weighted_zoo_rating,
        normalize_zoo_rating_input_for_kind, select_zoo_rating_input,
        smooth_zoo_rating_toward_target,
    },
    zoo_rating_input_types::{
        AnimalObservation, AnimalWelfareRating, GuestObservation, GuestRatingFacts,
        LivingAnimalPopulation, ZooFinanceRating, ZooOverallRatingTarget, ZooSceneryRating,
    },
};

pub(super) fn reduce_living_animal_population(
    living: Query<(&SpeciesHandle, Option<&NeedPoints>), (With<Animal>, Without<Dead>)>,
    changed: Query<
        (),
        (
            With<Animal>,
            Or<(
                Added<Animal>,
                Changed<SpeciesHandle>,
                Added<Dead>,
                Changed<NeedPoints>,
            )>,
        ),
    >,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_species: RemovedComponents<SpeciesHandle>,
    mut removed_dead: RemovedComponents<Dead>,
    mut removed_happiness: RemovedComponents<NeedPoints>,
    mut population: ResMut<LivingAnimalPopulation>,
) {
    let animals_removed = removed_animals.read().count() != 0;
    let species_removed = removed_species.read().count() != 0;
    let deaths_removed = removed_dead.read().count() != 0;
    let happiness_removed = removed_happiness.read().count() != 0;
    if !population.is_added()
        && changed.is_empty()
        && !animals_removed
        && !species_removed
        && !deaths_removed
        && !happiness_removed
    {
        return;
    }
    let mut species = BTreeSet::new();
    let mut observations = Vec::new();
    for (animal, happiness) in &living {
        species.insert(animal.species);
        observations.push(AnimalObservation {
            species: animal.species,
            happiness: happiness.map_or(0.0, |happiness| happiness.good),
        });
    }
    population.set_if_neq(LivingAnimalPopulation {
        animals: u32::try_from(observations.len()).unwrap_or(u32::MAX),
        species: u32::try_from(species.len()).unwrap_or(u32::MAX),
        observations,
    });
}

pub(super) fn recompute_animal_welfare_rating(
    animals: Query<&AnimalWelfare, (With<Animal>, Without<Dead>)>,
    changed: Query<
        (),
        (
            With<Animal>,
            Or<(Changed<AnimalWelfare>, Added<Animal>, Added<Dead>)>,
        ),
    >,
    mut removed_welfare: RemovedComponents<AnimalWelfare>,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_dead: RemovedComponents<Dead>,
    mut welfare: ResMut<AnimalWelfareRating>,
) {
    let welfare_removed = removed_welfare.read().count() != 0;
    let animals_removed = removed_animals.read().count() != 0;
    let deaths_removed = removed_dead.read().count() != 0;
    if welfare.is_added()
        || !changed.is_empty()
        || welfare_removed
        || animals_removed
        || deaths_removed
    {
        welfare.set_if_neq(AnimalWelfareRating(calculate_mean_permille(
            animals.iter().map(|value| value.0),
        )));
    }
}

pub(super) fn recompute_guest_ratings(
    guests: Query<
        (
            Option<&GuestSatisfaction>,
            Option<&GuestEducation>,
            Option<&GuestSpeciesViewCounts>,
            Option<&GuestSurveyExperience>,
            Option<&GuestSpeciesFameViews>,
        ),
        With<Guest>,
    >,
    changed: Query<
        (),
        (
            With<Guest>,
            Or<(
                Added<Guest>,
                Changed<GuestSatisfaction>,
                Changed<GuestEducation>,
                Changed<GuestSpeciesViewCounts>,
                Changed<GuestSurveyExperience>,
                Changed<GuestSpeciesFameViews>,
            )>,
        ),
    >,
    mut removed: (
        RemovedComponents<Guest>,
        RemovedComponents<GuestSatisfaction>,
        RemovedComponents<GuestEducation>,
        RemovedComponents<GuestSpeciesViewCounts>,
        RemovedComponents<GuestSurveyExperience>,
        RemovedComponents<GuestSpeciesFameViews>,
    ),
    mut facts: ResMut<GuestRatingFacts>,
) {
    let removed_count = removed.0.read().count()
        + removed.1.read().count()
        + removed.2.read().count()
        + removed.3.read().count()
        + removed.4.read().count()
        + removed.5.read().count();
    if !facts.is_added() && changed.is_empty() && removed_count == 0 {
        return;
    }
    let mut next = GuestRatingFacts::default();
    let mut satisfaction_totals = (0, 0);
    let mut education_totals = (0, 0);
    for (satisfaction, education, species_counts, survey, typed_views) in &guests {
        if let Some(value) = satisfaction {
            satisfaction_totals.0 += u64::from(value.0.min(1000));
            satisfaction_totals.1 += 1;
        }
        if let Some(value) = education {
            education_totals.0 += u64::from(value.0.min(1000));
            education_totals.1 += 1;
        }
        next.observations.push(GuestObservation {
            happiness: satisfaction.map_or(0.0, |value| value.source_q16() as f32 / 65_536.0),
            education: education.map_or(0.0, |value| f32::from(value.0) / 10.0),
        });
        if let Some(typed_views) = typed_views {
            for (&species, facts) in &typed_views.0 {
                let total = next.species_seen.entry(species).or_default();
                total.guests = total.guests.saturating_add(u32::from(facts.events != 0));
                total.events = total.events.saturating_add(facts.events);
                total.entertainment += facts.entertainment;
                total.donation += facts.donation;
            }
        } else {
            if let Some(counts) = species_counts {
                for &(species, count) in &counts.0 {
                    if count != 0 {
                        let total = next.species_seen.entry(species).or_default();
                        total.guests = total.guests.saturating_add(1);
                    }
                }
            }
            if let Some(survey) = survey {
                let total = next
                    .species_seen
                    .entry(openzt2_game_data::AssetId::default())
                    .or_default();
                total.events = total.events.saturating_add(survey.entertainment_views);
                total.entertainment += survey.entertainment_sum;
            }
        }
    }
    let mean = |(sum, count): (u64, u64)| ((sum + count / 2) / count.max(1)) as u16;
    next.count = u32::try_from(next.observations.len()).unwrap_or(u32::MAX);
    next.satisfaction = mean(satisfaction_totals);
    next.education = mean(education_totals);
    facts.set_if_neq(next);
}

pub(super) fn recompute_scenery_rating(
    mut removed: RemovedComponents<PlacedObjectDefinitionReference>,
    added: Query<(), Added<PlacedObjectDefinitionReference>>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    scenery: Query<(), With<PlacedObjectDefinitionReference>>,
    mut rating: ResMut<ZooSceneryRating>,
) {
    let removed = removed.read().count() != 0;
    if !assets.is_changed() && !definitions.is_changed() && added.is_empty() && !removed {
        return;
    }
    if let Some(definitions) = definitions.get(&assets) {
        let count = i64::try_from(scenery.iter().len()).unwrap_or(i64::MAX);
        rating.set_if_neq(ZooSceneryRating(normalize_zoo_rating_input_for_kind(
            definitions,
            &RatingInputKind::Scenery,
            count,
        )));
    }
}

pub(super) fn recompute_finance_rating(
    cash: Res<ZooCash>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut rating: ResMut<ZooFinanceRating>,
) {
    if !cash.is_changed() && !assets.is_changed() && !definitions.is_changed() {
        return;
    }
    if let Some(definitions) = definitions.get(&assets) {
        rating.set_if_neq(ZooFinanceRating(normalize_zoo_rating_input_for_kind(
            definitions,
            &RatingInputKind::Finance,
            cash.0 .0,
        )));
    }
}

pub(super) fn reconcile_zoo_rating_and_weighted_target(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    animals: Res<LivingAnimalPopulation>,
    welfare: Res<AnimalWelfareRating>,
    guests: Res<GuestRatingFacts>,
    scenery: Res<ZooSceneryRating>,
    finance: Res<ZooFinanceRating>,
    cleanliness: Res<ZooCleanlinessPermille>,
    mut rating: ResMut<ZooRating>,
    mut target: ResMut<ZooOverallRatingTarget>,
) {
    if !assets.is_changed()
        && !definitions.is_changed()
        && !animals.is_changed()
        && !welfare.is_changed()
        && !guests.is_changed()
        && !scenery.is_changed()
        && !finance.is_changed()
        && !cleanliness.is_changed()
    {
        return;
    }
    let view = definitions.get(&assets);
    let mut next = *rating;
    next.animal_welfare_permille = welfare.0;
    next.guest_satisfaction_permille = guests.satisfaction;
    next.education_permille = guests.education;
    if let Some(view) = view {
        next.variety_permille = normalize_zoo_rating_input_for_kind(
            view,
            &RatingInputKind::Variety,
            i64::from(animals.species),
        );
    }
    next.scenery_permille = scenery.0;
    next.finance_permille = finance.0;
    next.cleanliness_available = cleanliness.0.is_some();
    if let Some(value) = cleanliness.0 {
        next.cleanliness_permille = value.min(1000);
    }
    let weighted_target = view
        .and_then(|view| view.rating_definitions().next())
        .and_then(|definition| {
            let active_inputs = || definition.inputs.iter().filter(|input| input.weight > 0);
            if !active_inputs().all(|input| select_zoo_rating_input(&next, &input.kind).is_some()) {
                return None;
            }
            let value = calculate_weighted_zoo_rating(
                active_inputs().filter_map(|input| {
                    select_zoo_rating_input(&next, &input.kind).map(|value| (value, input.weight))
                }),
                definition.minimum,
                definition.maximum,
            );
            Some((value, definition.smoothing_ticks))
        });
    next.overall_available = weighted_target.is_some();
    target.set_if_neq(ZooOverallRatingTarget(weighted_target));
    rating.set_if_neq(next);
}

pub(super) fn advance_zoo_rating_smoothing(
    target: Res<ZooOverallRatingTarget>,
    mut rating: ResMut<ZooRating>,
) {
    if let Some((target, steps)) = target.0 {
        let next = smooth_zoo_rating_toward_target(rating.overall_permille, target, steps);
        if rating.overall_permille != next {
            rating.overall_permille = next;
        }
    }
}

pub(super) fn publish_zoo_rating_changes(
    rating: Res<ZooRating>,
    mut changes: MessageWriter<ProgressionFactChanged>,
) {
    if rating.is_changed() {
        changes.write(ProgressionFactChanged::Rating);
    }
}

pub(super) fn reset_rating_inputs_after_leaving_game(
    mut animals: ResMut<LivingAnimalPopulation>,
    mut welfare: ResMut<AnimalWelfareRating>,
    mut guests: ResMut<GuestRatingFacts>,
    mut scenery: ResMut<ZooSceneryRating>,
    mut finance: ResMut<ZooFinanceRating>,
    mut target: ResMut<ZooOverallRatingTarget>,
) {
    animals.set_if_neq(default());
    welfare.set_if_neq(default());
    guests.set_if_neq(default());
    scenery.set_if_neq(default());
    finance.set_if_neq(default());
    target.set_if_neq(default());
}

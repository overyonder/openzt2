use std::collections::BTreeSet;

use bevy::{ecs::entity::EntityHashMap, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::RatingInputKind;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};
use openzt2_zoo_state::animal_welfare::types::{AnimalWelfare, NeedPoints};
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::guests::{
    guest_simulation_types::{Guest, GuestEducation, GuestSatisfaction, GuestSpeciesViewCounts},
    guest_survey_types::GuestSurveyExperience,
};
use openzt2_zoo_state::maintenance::maintenance_types::ZooCleanlinessPermille;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::progression::fame_types::GuestSpeciesFameViews;

use super::{
    award_and_progression_fact_types::{EarnedProgressionAward, ProgressionFactChanged},
    rating_types::ZooRating,
    zoo_rating_calculations::{
        calculate_weighted_zoo_rating, normalize_zoo_rating_input_for_kind,
        select_zoo_rating_input, smooth_zoo_rating_toward_target,
    },
    zoo_rating_input_types::{
        AnimalObservation, AnimalWelfareRating, EarnedAwardFamePoints, GuestObservation,
        GuestRatingFacts, LivingAnimalPopulation, ZooFinanceRating, ZooOverallRatingTarget,
        ZooSceneryRating,
    },
};

pub(super) fn reduce_living_animal_population(
    living: Query<
        (&WorldMember, &SpeciesHandle, Option<&NeedPoints>),
        (With<Animal>, Without<Dead>),
    >,
    changed: Query<
        (),
        (
            With<Animal>,
            Or<(
                Added<Animal>,
                Changed<WorldMember>,
                Changed<SpeciesHandle>,
                Added<Dead>,
                Changed<NeedPoints>,
            )>,
        ),
    >,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut removed_species: RemovedComponents<SpeciesHandle>,
    mut removed_dead: RemovedComponents<Dead>,
    mut removed_happiness: RemovedComponents<NeedPoints>,
    mut worlds: Query<(Entity, &mut LivingAnimalPopulation), With<WorldRoot>>,
) {
    let removals = removed_animals.read().count()
        + removed_members.read().count()
        + removed_species.read().count()
        + removed_dead.read().count()
        + removed_happiness.read().count();
    if removals == 0 && changed.is_empty() {
        return;
    }
    let mut populations = EntityHashMap::<(BTreeSet<_>, Vec<_>)>::default();
    for (member, species, happiness) in &living {
        let (species_seen, observations) = populations.entry(member.root).or_default();
        species_seen.insert(species.species);
        observations.push(AnimalObservation {
            species: species.species,
            happiness: happiness.map_or(0.0, |happiness| happiness.good),
        });
    }
    for (world, mut population) in &mut worlds {
        let (species, observations) = populations.remove(&world).unwrap_or_default();
        population.set_if_neq(LivingAnimalPopulation {
            animals: u32::try_from(observations.len()).unwrap_or(u32::MAX),
            species: u32::try_from(species.len()).unwrap_or(u32::MAX),
            observations,
        });
    }
}

pub(super) fn recompute_animal_welfare_rating(
    animals: Query<(&WorldMember, &AnimalWelfare), (With<Animal>, Without<Dead>)>,
    changed: Query<
        (),
        (
            With<Animal>,
            Or<(
                Changed<AnimalWelfare>,
                Changed<WorldMember>,
                Added<Animal>,
                Added<Dead>,
            )>,
        ),
    >,
    mut removed_welfare: RemovedComponents<AnimalWelfare>,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut removed_dead: RemovedComponents<Dead>,
    mut worlds: Query<(Entity, &mut AnimalWelfareRating), With<WorldRoot>>,
) {
    let removals = removed_welfare.read().count()
        + removed_animals.read().count()
        + removed_members.read().count()
        + removed_dead.read().count();
    if removals == 0 && changed.is_empty() {
        return;
    }
    let mut by_world = EntityHashMap::<(u64, u64)>::default();
    for (member, value) in &animals {
        let (sum, count) = by_world.entry(member.root).or_default();
        *sum += u64::from(value.0.min(1000));
        *count += 1;
    }
    for (world, mut welfare) in &mut worlds {
        let (sum, count) = by_world.remove(&world).unwrap_or_default();
        welfare.set_if_neq(AnimalWelfareRating(mean_permille(sum, count)));
    }
}

fn mean_permille(sum: u64, count: u64) -> u16 {
    ((sum + count / 2) / count.max(1)) as u16
}

pub(super) fn recompute_guest_ratings(
    guests: Query<
        (
            &WorldMember,
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
                Changed<WorldMember>,
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
    mut removed_members: RemovedComponents<WorldMember>,
    mut worlds: Query<(Entity, &mut GuestRatingFacts), With<WorldRoot>>,
) {
    let removed_count = removed.0.read().count()
        + removed.1.read().count()
        + removed.2.read().count()
        + removed.3.read().count()
        + removed.4.read().count()
        + removed.5.read().count()
        + removed_members.read().count();
    if changed.is_empty() && removed_count == 0 {
        return;
    }
    let mut by_world = EntityHashMap::<(GuestRatingFacts, (u64, u64), (u64, u64))>::default();
    for (member, satisfaction, education, species_counts, survey, typed_views) in &guests {
        let (next, satisfaction_totals, education_totals) =
            by_world.entry(member.root).or_default();
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
    for (world, mut facts) in &mut worlds {
        let (mut next, (satisfaction_sum, satisfaction_count), (education_sum, education_count)) =
            by_world.remove(&world).unwrap_or_default();
        next.count = u32::try_from(next.observations.len()).unwrap_or(u32::MAX);
        next.satisfaction = mean_permille(satisfaction_sum, satisfaction_count);
        next.education = mean_permille(education_sum, education_count);
        facts.set_if_neq(next);
    }
}

pub(super) fn count_earned_award_fame_points(
    awards: Query<&WorldMember, With<EarnedProgressionAward>>,
    changed: Query<
        (),
        (
            With<EarnedProgressionAward>,
            Or<(Added<EarnedProgressionAward>, Changed<WorldMember>)>,
        ),
    >,
    mut removed_awards: RemovedComponents<EarnedProgressionAward>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut worlds: Query<(Entity, &mut EarnedAwardFamePoints), With<WorldRoot>>,
) {
    let removals = removed_awards.read().count() + removed_members.read().count();
    if removals == 0 && changed.is_empty() {
        return;
    }
    let mut counts = EntityHashMap::<u32>::default();
    for member in &awards {
        let count = counts.entry(member.root).or_default();
        *count = count.saturating_add(1);
    }
    for (world, mut points) in &mut worlds {
        points.set_if_neq(EarnedAwardFamePoints(
            counts.remove(&world).unwrap_or_default(),
        ));
    }
}

pub(super) fn recompute_scenery_rating(
    mut removed_objects: RemovedComponents<PlacedObjectDefinitionReference>,
    mut removed_members: RemovedComponents<WorldMember>,
    changed: Query<
        (),
        (
            With<PlacedObjectDefinitionReference>,
            Or<(Added<PlacedObjectDefinitionReference>, Changed<WorldMember>)>,
        ),
    >,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    scenery: Query<&WorldMember, With<PlacedObjectDefinitionReference>>,
    added_roots: Query<(), Added<WorldRoot>>,
    mut worlds: Query<(Entity, &mut ZooSceneryRating), With<WorldRoot>>,
) {
    let removals = removed_objects.read().count() + removed_members.read().count();
    if !assets.is_changed()
        && !definitions.is_changed()
        && added_roots.is_empty()
        && changed.is_empty()
        && removals == 0
    {
        return;
    }
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let mut counts = EntityHashMap::<i64>::default();
    for member in &scenery {
        *counts.entry(member.root).or_default() += 1;
    }
    for (world, mut rating) in &mut worlds {
        rating.set_if_neq(ZooSceneryRating(normalize_zoo_rating_input_for_kind(
            definitions,
            &RatingInputKind::Scenery,
            counts.remove(&world).unwrap_or_default(),
        )));
    }
}

pub(super) fn recompute_finance_rating(
    cash: Option<Single<Ref<ZooCash>>>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    added_roots: Query<(), Added<WorldRoot>>,
    completed_roots: Query<(), Added<WorldLoadCompleted>>,
    mut worlds: Query<(Entity, &mut ZooFinanceRating), With<WorldRoot>>,
) {
    let cash = cash.map(Single::into_inner);
    if !cash.as_ref().is_some_and(|cash| cash.is_changed())
        && completed_roots.is_empty()
        && !assets.is_changed()
        && !definitions.is_changed()
        && added_roots.is_empty()
    {
        return;
    }
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let active_world = active_world.single().ok();
    for (world, mut rating) in &mut worlds {
        let balance = cash.as_deref().filter(|_| active_world == Some(world));
        rating.set_if_neq(ZooFinanceRating(balance.map(|cash| {
            normalize_zoo_rating_input_for_kind(definitions, &RatingInputKind::Finance, cash.0 .0)
        })));
    }
}

pub(super) fn reconcile_zoo_rating_and_weighted_target(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    completed_roots: Query<(), Added<WorldLoadCompleted>>,
    mut worlds: Query<
        (
            Entity,
            Ref<LivingAnimalPopulation>,
            Ref<AnimalWelfareRating>,
            Ref<GuestRatingFacts>,
            Ref<ZooSceneryRating>,
            Ref<ZooFinanceRating>,
            Option<Ref<ZooCleanlinessPermille>>,
            &mut ZooOverallRatingTarget,
            &mut ZooRating,
        ),
        With<WorldRoot>,
    >,
) {
    let definitions_changed = assets.is_changed() || definitions.is_changed();
    let view = definitions.get(&assets);
    let active_world = active_world.single().ok();
    for (
        world,
        animals,
        welfare,
        guests,
        scenery,
        finance,
        cleanliness,
        mut target,
        mut world_rating,
    ) in &mut worlds
    {
        let active = active_world == Some(world);
        if !definitions_changed
            && !animals.is_changed()
            && !welfare.is_changed()
            && !guests.is_changed()
            && !scenery.is_changed()
            && !finance.is_changed()
            && !cleanliness.as_ref().is_some_and(|value| value.is_changed())
            && !(active && completed_roots.contains(world))
        {
            continue;
        }
        let mut next = *world_rating;
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
        next.finance_permille = finance.0.unwrap_or_default();
        let cleanliness = cleanliness.and_then(|value| value.0);
        next.cleanliness_available = cleanliness.is_some();
        if let Some(value) = cleanliness {
            next.cleanliness_permille = value.min(1000);
        }
        let weighted_target = view
            .and_then(|view| view.rating_definitions().next())
            .and_then(|definition| {
                let active_inputs = || definition.inputs.iter().filter(|input| input.weight > 0);
                if !active_inputs().all(|input| {
                    (input.kind != RatingInputKind::Finance || finance.0.is_some())
                        && select_zoo_rating_input(&next, &input.kind).is_some()
                }) {
                    return None;
                }
                let value = calculate_weighted_zoo_rating(
                    active_inputs().filter_map(|input| {
                        select_zoo_rating_input(&next, &input.kind)
                            .map(|value| (value, input.weight))
                    }),
                    definition.minimum,
                    definition.maximum,
                );
                Some((value, definition.smoothing_ticks))
            });
        next.overall_available = weighted_target.is_some();
        target.set_if_neq(ZooOverallRatingTarget(weighted_target));
        world_rating.set_if_neq(next);
    }
}

pub(super) fn advance_zoo_rating_smoothing(
    mut worlds: Query<(&ZooOverallRatingTarget, &mut ZooRating), With<WorldRoot>>,
) {
    for (target, mut world_rating) in &mut worlds {
        let Some((target, steps)) = target.0 else {
            continue;
        };
        let current = world_rating.overall_permille;
        let next = smooth_zoo_rating_toward_target(current, target, steps);
        if world_rating.overall_permille != next {
            world_rating.overall_permille = next;
        }
    }
}

pub(super) fn publish_zoo_rating_changes(
    ratings: Query<Ref<ZooRating>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut changes: MessageWriter<ProgressionFactChanged>,
) {
    if ratings.single().is_ok_and(|rating| rating.is_changed()) {
        changes.write(ProgressionFactChanged::Rating);
    }
}

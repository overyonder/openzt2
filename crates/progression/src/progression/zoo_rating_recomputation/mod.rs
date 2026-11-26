use std::collections::BTreeSet;

use bevy::ecs::{entity_disabling::Disabled, query::Allow};

use bevy::{ecs::{entity::{EntityHashMap, EntityHashSet}, relationship::RelationshipTarget}, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::RatingInputKind;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldMembers, WorldRoot},
};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle};
use openzt2_zoo_state::animal_welfare::types::AnimalWelfare;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::guests::guest_simulation_types::{Guest, GuestEducation, GuestSatisfaction};
use openzt2_zoo_state::maintenance::maintenance_types::ZooCleanlinessPermille;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    rating_types::ZooRating,
    zoo_rating_calculations::{
        calculate_weighted_zoo_rating, normalize_zoo_rating_input_for_kind,
        select_zoo_rating_input, smooth_zoo_rating_toward_target,
    },
    zoo_rating_input_types::{
        AnimalWelfareRating, GuestRatingFacts, LivingAnimalPopulation, ZooFinanceRating,
        ZooOverallRatingTarget, ZooSceneryRating,
    },
};

pub(super) fn reduce_living_animal_population_and_welfare(
    animals: Query<(&SpeciesHandle, Option<&AnimalWelfare>), (With<Animal>, Without<Dead>)>,
    changes: Query<
        &WorldMember,
        (
            With<Animal>,
            Or<(
                Added<Animal>,
                Changed<SpeciesHandle>,
                Changed<AnimalWelfare>,
                Added<Dead>,
            )>,
        ),
    >,
    memberships: Query<&WorldMember>,
    mut retired: (
        RemovedComponents<Animal>,
        RemovedComponents<SpeciesHandle>,
        RemovedComponents<AnimalWelfare>,
        RemovedComponents<Dead>,
        RemovedComponents<WorldMembers>,
    ),
    mut roots: Query<
        (
            Entity,
            Option<Ref<WorldMembers>>,
            &mut LivingAnimalPopulation,
            &mut AnimalWelfareRating,
        ),
        With<WorldRoot>,
    >,
) {
    let affected: EntityHashSet = changes
        .iter()
        .map(|member| member.root)
        .chain(
            memberships
                .iter_many(
                    retired
                        .0
                        .read()
                        .chain(retired.1.read())
                        .chain(retired.2.read())
                        .chain(retired.3.read()),
                )
                .flatten()
                .map(|member| member.root),
        )
        .chain(retired.4.read())
        .collect();
    for (root, members, mut population, mut welfare) in &mut roots {
        if !population.is_added()
            && !affected.contains(&root)
            && !members.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let mut species = BTreeSet::new();
        let mut animal_count = 0_u32;
        let (mut sum, mut count) = (0_u64, 0_u64);
        for (identity, value) in animals
            .iter_many(
                members
                    .as_deref()
                    .into_iter()
                    .flat_map(RelationshipTarget::iter),
            )
            .flatten()
        {
            animal_count = animal_count.saturating_add(1);
            species.insert(identity.species);
            if let Some(value) = value {
                sum += u64::from(value.0.min(1000));
                count += 1;
            }
        }
        population.set_if_neq(LivingAnimalPopulation {
            animals: animal_count,
            species: u32::try_from(species.len()).unwrap_or(u32::MAX),
        });
        welfare.set_if_neq(AnimalWelfareRating(mean_permille(sum, count)));
    }
}

fn mean_permille(sum: u64, count: u64) -> u16 {
    ((sum + count / 2) / count.max(1)) as u16
}

#[derive(Component, PartialEq, Eq)]
pub(super) struct GuestRatingContribution {
    root: Entity,
    satisfaction: Option<u16>,
    education: Option<u16>,
}

#[derive(Component, Default)]
pub(super) struct GuestRatingSums {
    guests: u32,
    satisfaction: u64,
    satisfied_guests: u64,
    education: u64,
    educated_guests: u64,
}

impl GuestRatingSums {
    fn add(&mut self, contribution: &GuestRatingContribution) {
        self.guests = self.guests.saturating_add(1);
        if let Some(value) = contribution.satisfaction {
            self.satisfaction += u64::from(value);
            self.satisfied_guests += 1;
        }
        if let Some(value) = contribution.education {
            self.education += u64::from(value);
            self.educated_guests += 1;
        }
    }

    fn subtract(&mut self, contribution: &GuestRatingContribution) {
        self.guests = self.guests.saturating_sub(1);
        if let Some(value) = contribution.satisfaction {
            self.satisfaction = self.satisfaction.saturating_sub(u64::from(value));
            self.satisfied_guests = self.satisfied_guests.saturating_sub(1);
        }
        if let Some(value) = contribution.education {
            self.education = self.education.saturating_sub(u64::from(value));
            self.educated_guests = self.educated_guests.saturating_sub(1);
        }
    }
}

pub(super) fn withdraw_retired_guest_rating_terms(
    retired: On<Discard<GuestRatingContribution>>,
    contributions: Query<&GuestRatingContribution, Allow<Disabled>>,
    mut sums: Query<&mut GuestRatingSums, Allow<Disabled>>,
) {
    if let Ok(contribution) = contributions.get(retired.entity) {
        if let Ok(mut current) = sums.get_mut(contribution.root) {
            current.subtract(contribution);
        }
    }
}

pub(super) fn reconcile_changed_guest_rating_terms(
    changed: Query<
        Entity,
        (
            With<Guest>,
            Allow<Disabled>,
            Or<(
                Changed<GuestSatisfaction>,
                Changed<GuestEducation>,
                Changed<WorldMember>,
                Added<Guest>,
                Added<Disabled>,
            )>,
        ),
    >,
    mut guests: Query<
        (
            Has<Guest>,
            Has<Disabled>,
            Option<&WorldMember>,
            Option<&GuestSatisfaction>,
            Option<&GuestEducation>,
            Option<&mut GuestRatingContribution>,
        ),
        (
            Allow<Disabled>,
            Or<(With<Guest>, With<GuestRatingContribution>)>,
        ),
    >,
    mut sums: Query<&mut GuestRatingSums>,
    mut retired: (
        RemovedComponents<Guest>,
        RemovedComponents<WorldMember>,
        RemovedComponents<GuestSatisfaction>,
        RemovedComponents<GuestEducation>,
        RemovedComponents<Disabled>,
    ),
    mut commands: Commands,
) {
    let dirty: EntityHashSet = changed
        .iter()
        .chain(retired.0.read())
        .chain(retired.1.read())
        .chain(retired.2.read())
        .chain(retired.3.read())
        .chain(retired.4.read())
        .collect();
    for entity in dirty {
        let Ok((guest, disabled, membership, satisfaction, education, contribution)) =
            guests.get_mut(entity)
        else {
            continue;
        };
        let Some(member) = membership.filter(|_| guest && !disabled) else {
            if contribution.is_some() {
                commands.entity(entity).remove::<GuestRatingContribution>();
            }
            continue;
        };
        let next = GuestRatingContribution {
            root: member.root,
            satisfaction: satisfaction.map(|value| value.0.min(1000)),
            education: education.map(|value| value.0.min(1000)),
        };
        if let Some(mut contribution) = contribution {
            if *contribution == next {
                continue;
            }
            if let Ok(mut previous) = sums.get_mut(contribution.root) {
                previous.subtract(&contribution);
            }
            if let Ok(mut current) = sums.get_mut(next.root) {
                current.add(&next);
            }
            contribution.set_if_neq(next);
        } else {
            if let Ok(mut current) = sums.get_mut(next.root) {
                current.add(&next);
                commands.entity(entity).insert(next);
            }
        }
    }
}

pub(super) fn publish_guest_rating_means(
    mut worlds: Query<
        (&GuestRatingSums, &mut GuestRatingFacts),
        (With<WorldRoot>, Changed<GuestRatingSums>),
    >,
) {
    for (sums, mut facts) in &mut worlds {
        facts.set_if_neq(GuestRatingFacts {
            count: sums.guests,
            satisfaction: mean_permille(sums.satisfaction, sums.satisfied_guests),
            education: mean_permille(sums.education, sums.educated_guests),
        });
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
            normalize_zoo_rating_input_for_kind(
                definitions,
                &RatingInputKind::Finance,
                (f64::from(cash.0.dollars()) * 100.0).round() as i64,
            )
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

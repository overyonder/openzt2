use std::collections::BTreeSet;
use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::RatingInputKind;

use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::AnimalDied;
use crate::plugins::animal_health::types::Dead;
use crate::plugins::animal_lifecycle::animal_adoption_contracts::AnimalAdopted;
use crate::plugins::animal_lifecycle::animal_birth_and_release_contracts::AnimalBorn;
use crate::plugins::animal_lifecycle::animal_birth_and_release_contracts::AnimalReleased;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::animal_welfare::types::AnimalWelfare;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::guests::guest_simulation_types::GuestArrived;
use crate::plugins::guests::guest_simulation_types::GuestDeparted;
use crate::plugins::guests::guest_simulation_types::GuestEducation;
use crate::plugins::guests::guest_simulation_types::GuestReaction;
use crate::plugins::guests::guest_simulation_types::GuestSatisfaction;
use crate::plugins::maintenance::maintenance_types::ZooCleanlinessPermille;
use crate::plugins::placement::placed_object_types::PlacedObjectDefinitionReference;
use crate::plugins::placement::placement_transaction_types::ObjectPlacementCommitted;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    rating_types::ZooRating,
    zoo_rating_calculations::{
        calculate_mean_permille, calculate_weighted_zoo_rating,
        normalize_zoo_rating_input_for_kind, select_zoo_rating_input,
        smooth_zoo_rating_toward_target,
    },
};

fn message_reader_contains_any_unread_message<T: Message>(
    message_reader: &mut MessageReader<T>,
) -> bool {
    let mut contains_unread_message = false;
    for _ in message_reader.read() {
        contains_unread_message = true;
    }
    contains_unread_message
}

fn update_zoo_rating_field_and_publish_change(
    rating_field: &mut u16,
    next_value: u16,
    changed_progression_facts: &mut MessageWriter<ProgressionFactChanged>,
) {
    if *rating_field != next_value {
        *rating_field = next_value;
        changed_progression_facts.write(ProgressionFactChanged::Rating);
    }
}

pub(super) fn recompute_animal_welfare_rating(
    changed_welfare: Query<(), (With<Animal>, Without<Dead>, Changed<AnimalWelfare>)>,
    mut removed_welfare: RemovedComponents<AnimalWelfare>,
    mut released: MessageReader<AnimalReleased>,
    mut died: MessageReader<AnimalDied>,
    animals: Query<&AnimalWelfare, (With<Animal>, Without<Dead>)>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let removed = removed_welfare.read().count() != 0;
    let released = message_reader_contains_any_unread_message(&mut released);
    let died = message_reader_contains_any_unread_message(&mut died);
    if changed_welfare.is_empty() && !removed && !released && !died {
        return;
    }
    update_zoo_rating_field_and_publish_change(
        &mut rating.animal_welfare_permille,
        calculate_mean_permille(animals.iter().map(|value| value.0)),
        &mut changed,
    );
}

pub(super) fn recompute_guest_satisfaction_rating(
    mut arrived: MessageReader<GuestArrived>,
    mut departed: MessageReader<GuestDeparted>,
    mut reactions: MessageReader<GuestReaction>,
    guests: Query<&GuestSatisfaction, With<Guest>>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !message_reader_contains_any_unread_message(&mut arrived)
        && !message_reader_contains_any_unread_message(&mut departed)
        && !message_reader_contains_any_unread_message(&mut reactions)
    {
        return;
    }
    update_zoo_rating_field_and_publish_change(
        &mut rating.guest_satisfaction_permille,
        calculate_mean_permille(guests.iter().map(|value| value.0)),
        &mut changed,
    );
}

pub(super) fn recompute_education_rating(
    mut arrived: MessageReader<GuestArrived>,
    mut departed: MessageReader<GuestDeparted>,
    mut reactions: MessageReader<GuestReaction>,
    guests: Query<&GuestEducation, With<Guest>>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !message_reader_contains_any_unread_message(&mut arrived)
        && !message_reader_contains_any_unread_message(&mut departed)
        && !message_reader_contains_any_unread_message(&mut reactions)
    {
        return;
    }
    update_zoo_rating_field_and_publish_change(
        &mut rating.education_permille,
        calculate_mean_permille(guests.iter().map(|value| value.0)),
        &mut changed,
    );
}

pub(super) fn recompute_variety_rating(
    mut adopted: MessageReader<AnimalAdopted>,
    mut born: MessageReader<AnimalBorn>,
    mut released: MessageReader<AnimalReleased>,
    mut died: MessageReader<AnimalDied>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    animals: Query<&SpeciesHandle, (With<Animal>, Without<Dead>)>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !message_reader_contains_any_unread_message(&mut adopted)
        && !message_reader_contains_any_unread_message(&mut born)
        && !message_reader_contains_any_unread_message(&mut released)
        && !message_reader_contains_any_unread_message(&mut died)
    {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let unique = animals
        .iter()
        .map(|animal| animal.species)
        .collect::<BTreeSet<_>>()
        .len() as u32;
    let value = normalize_zoo_rating_input_for_kind(
        definitions,
        &RatingInputKind::Variety,
        i64::from(unique),
    );
    update_zoo_rating_field_and_publish_change(&mut rating.variety_permille, value, &mut changed);
}

pub(super) fn recompute_scenery_rating(
    mut placed: MessageReader<ObjectPlacementCommitted>,
    mut removed: RemovedComponents<PlacedObjectDefinitionReference>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    scenery: Query<&PlacedObjectDefinitionReference>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let placement_changed =
        message_reader_contains_any_unread_message(&mut placed) || removed.read().next().is_some();
    if !placement_changed {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let value = normalize_zoo_rating_input_for_kind(
        definitions,
        &RatingInputKind::Scenery,
        scenery.iter().count() as i64,
    );
    update_zoo_rating_field_and_publish_change(&mut rating.scenery_permille, value, &mut changed);
}

pub(super) fn recompute_finance_rating(
    cash: Res<ZooCash>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !cash.is_changed() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let value =
        normalize_zoo_rating_input_for_kind(definitions, &RatingInputKind::Finance, cash.0 .0);
    update_zoo_rating_field_and_publish_change(&mut rating.finance_permille, value, &mut changed);
}

pub(super) fn recompute_cleanliness_rating(
    cleanliness: Res<ZooCleanlinessPermille>,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !cleanliness.is_changed() && !rating.is_changed() {
        return;
    }
    let available = cleanliness.0.is_some();
    if rating.cleanliness_available != available {
        rating.cleanliness_available = available;
        changed.write(ProgressionFactChanged::Rating);
    }
    if let Some(value) = cleanliness.0 {
        update_zoo_rating_field_and_publish_change(
            &mut rating.cleanliness_permille,
            value.min(1000),
            &mut changed,
        );
    }
}

pub(super) fn recompute_overall_rating(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    simulation_control: Option<
        Res<crate::plugins::simulation_time::simulation_control_types::SimulationControl>,
    >,
    mut rating: ResMut<ZooRating>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if !rating.is_changed()
        && !active_definitions.is_changed()
        && !definitions.is_changed()
        && !simulation_control
            .as_ref()
            .is_some_and(|control| control.is_changed())
    {
        return;
    }
    let definitions = active_definitions.get(&definitions);
    let definition = definitions
        .as_ref()
        .and_then(|definitions| definitions.rating_definitions().next());
    let available = definition.is_some_and(|definition| {
        definition
            .inputs
            .iter()
            .filter(|input| input.weight > 0)
            .all(|input| select_zoo_rating_input(&rating, &input.kind).is_some())
    });
    if rating.overall_available != available {
        rating.overall_available = available;
        changed.write(ProgressionFactChanged::Rating);
    }
    let Some(definition) = definition.filter(|_| available) else {
        return;
    };
    if !crate::plugins::simulation_time::simulation_control_application::simulation_is_running(
        simulation_control,
    ) {
        return;
    }
    let target = calculate_weighted_zoo_rating(
        definition
            .inputs
            .iter()
            .filter(|input| input.weight > 0)
            .filter_map(|input| {
                select_zoo_rating_input(&rating, &input.kind).map(|value| (value, input.weight))
            }),
        definition.minimum,
        definition.maximum,
    );
    let next = smooth_zoo_rating_toward_target(
        rating.overall_permille,
        target,
        definition.smoothing_ticks,
    );
    if rating.overall_permille != next {
        rating.overall_permille = next;
        changed.write(ProgressionFactChanged::Rating);
    }
}

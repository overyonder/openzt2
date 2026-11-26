use crate::guests::view_event_execution::AppliedAnimalView;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_lifecycle::types::SpeciesHandle,
    donations::donation_guest_decision_types::GuestDonationMeters,
    guests::{
        guest_simulation_calculations::animal_view_distance_factor,
        guest_simulation_types::{GuestArchetype, GuestFavouriteAnimal},
    },
};
use std::collections::BTreeMap;

#[derive(Component, Default)]
pub(crate) struct SpeciesDonationCandidateLifetimes(pub(crate) BTreeMap<AssetId, f32>);

pub(crate) const SPENT_SPECIES_SCORE: f32 = -1_000_000.0;

pub(super) fn accumulate_signed_species_donation_scores(
    mut reactions: MessageReader<AppliedAnimalView>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut guests: Query<(
        &GuestArchetype,
        &GuestFavouriteAnimal,
        &WorldMember,
        &GlobalTransform,
        &mut GuestDonationMeters,
    )>,
    animals: Query<(&SpeciesHandle, &WorldMember, &GlobalTransform)>,
) {
    let Some(definitions) = active.get(&definitions) else {
        reactions.clear();
        return;
    };
    let Some(viewing) = definitions.guest_viewing() else {
        reactions.clear();
        return;
    };
    for reaction in reactions.read() {
        let Ok((archetype, favourite, world, position, mut meters)) =
            guests.get_mut(reaction.guest)
        else {
            continue;
        };
        let Ok((species, animal_world, animal_position)) = animals.get(reaction.subject) else {
            continue;
        };
        if world.root != animal_world.root {
            continue;
        }
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let preferred_factor = if favourite.0 == Some(species.species) {
            definition
                .donation
                .map_or(1.0, |policy| policy.preferred_animal_factor)
        } else {
            1.0
        };
        let distance_factor = animal_view_distance_factor(
            definition.view_scoring,
            position
                .translation()
                .distance(animal_position.translation()),
            viewing.ground_range_m,
        );
        let meter = meters.0.entry(species.species).or_default();
        if *meter != SPENT_SPECIES_SCORE {
            *meter += reaction.donation_score * preferred_factor * distance_factor;
        }
    }
}

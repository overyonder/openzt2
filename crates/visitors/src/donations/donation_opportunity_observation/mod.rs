use crate::guests::view_event_execution::AppliedAnimalView;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_lifecycle::types::{AnimalVariant, SpeciesHandle},
    authored_float_attributes::AuthoredFloatAttributes,
    donations::donation_guest_decision_types::{
        GuestDonationMeters, SpeciesDonationCandidate, SpeciesDonationCandidateLifetimes,
    },
    guests::{
        guest_simulation_calculations::animal_view_distance_factor,
        guest_simulation_types::{GuestArchetype, GuestFavouriteAnimal},
    },
};

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
        &mut SpeciesDonationCandidateLifetimes,
        Option<&AuthoredFloatAttributes>,
    )>,
    animals: Query<(
        &SpeciesHandle,
        Option<&AnimalVariant>,
        &WorldMember,
        &GlobalTransform,
    )>,
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
        let Ok((archetype, favourite, world, position, mut meters, mut candidates, attributes)) =
            guests.get_mut(reaction.guest)
        else {
            continue;
        };
        let Ok((species, variant, animal_world, animal_position)) = animals.get(reaction.subject)
        else {
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
        let binder = variant.map_or(species.species, |variant| variant.0);
        let maximum = view_property(definitions, archetype.0, attributes, "speciesmeters")
            .unwrap_or(150.0)
            .max(0.0);
        let preferred_view = if favourite.0 == Some(species.species) {
            definition.preferred_animal_view_factor_q16 as f32 / 65_536.0
        } else {
            1.0
        };
        let candidate = candidates
            .0
            .entry(species.species)
            .or_insert(SpeciesDonationCandidate {
                binder,
                remaining: maximum,
            });
        candidate.binder = binder;
        candidate.remaining = (candidate.remaining
            + reaction.view_score * preferred_view * distance_factor)
            .clamp(0.0, maximum);
        let meter = meters.0.entry(species.species).or_default();
        if *meter != SPENT_SPECIES_SCORE {
            *meter += reaction.donation_score * preferred_factor * distance_factor;
        }
    }
}

fn view_property(
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    archetype: AssetId,
    attributes: Option<&AuthoredFloatAttributes>,
    property: &str,
) -> Option<f32> {
    let key = AssetId::from_key(property);
    attributes
        .and_then(|attributes| attributes.get(key))
        .or_else(|| {
            let object = definitions.find_guest(archetype)?.object;
            definitions
                .find_object(object)?
                .authored_runtime
                .numeric_behavior_properties
                .iter()
                .find_map(|&(property, value)| (property == key).then_some(value))
        })
}

pub(super) fn decay_live_species_donation_candidates(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    time: Res<Time<Fixed>>,
    mut guests: Query<(
        &GuestArchetype,
        &mut SpeciesDonationCandidateLifetimes,
        Option<&AuthoredFloatAttributes>,
    )>,
) {
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    for (archetype, mut candidates, attributes) in &mut guests {
        if candidates.0.is_empty() {
            continue;
        }
        let rate = view_property(definitions, archetype.0, attributes, "f_viewspeedbyspecies")
            .unwrap_or(1.0);
        let decrement = rate * time.delta_secs();
        if decrement == 0.0 {
            continue;
        }
        for candidate in candidates.0.values_mut() {
            candidate.remaining = (candidate.remaining - decrement).max(0.0);
        }
    }
}

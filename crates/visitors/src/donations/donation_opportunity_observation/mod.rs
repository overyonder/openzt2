use crate::guests::view_event_execution::AppliedAnimalView;
use openzt2_zoo_state::donations::donation_guest_decision_types::GuestDonationMeters;
use openzt2_zoo_state::donations::donation_guest_decision_types::{DonationObservation, DonationCandidate, DonationProgress};
use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{AssetId};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, SpeciesHandle, AnimalVariant};
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::guests::guest_simulation_types::{Guest, GuestArchetype, GuestFavouriteAnimal, GuestPhase, Viewing};
use super::{donation_opportunity_types::DonationOpportunity};
use openzt2_zoo_state::donations::donation_payment_types::DonationPaymentRequested;

pub(super) fn hydrate_animal_donation_opportunities(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    animals: Query<
        (
            Entity,
            &SpeciesHandle,
            &AnimalVariant,
            Option<&DonationOpportunity>,
            Has<Dead>,
            Has<WorldEntityIsCrated>,
        ),
        With<Animal>,
    >,
) {
    let Some(species) = active_species.get(&species_assets) else {
        return;
    };
    for (entity, handle, variant, current, dead, crated) in &animals {
        let definition = species.find(handle.species);
        let policy = definition.and_then(|definition| definition.donation);
        if dead || crated || policy.is_none() {
            if current.is_some() {
                commands.entity(entity).remove::<DonationOpportunity>();
            }
            continue;
        }
        let (Some(definition), Some(policy)) = (definition, policy) else {
            continue;
        };
        let name = species
            .find_variant_binding(variant.0)
            .and_then(|binding| binding.authored_species_name_override.as_deref())
            .unwrap_or(&definition.authored_species_name);
        let next = DonationOpportunity {
            category: AssetId::from_key(&format!("Donate_{name}")),
            amount: Money(i64::from(policy.base_amount_cents)),
            interaction_ticks: 0,
        };
        if current != Some(&next) {
            commands.entity(entity).insert(next);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn observe_donation_opportunities_after_applied_viewing_reactions(
    mut reactions: MessageReader<AppliedAnimalView>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    mut guests: Query<
        (
            &Viewing,
            &GuestPhase,
            &WorldMember,
            &GuestArchetype,
            &GuestFavouriteAnimal,
            &mut PendingDonationObservation,
            &mut GuestDonationMeters,
            &GlobalTransform,
        ),
        (
            With<Guest>,
            Without<DonationCandidate>,
            Without<DonationProgress>,
            Without<DonationPaymentRequested>,
        ),
    >,
    opportunities: Query<(
        &DonationOpportunity,
        &WorldMember,
        &SpeciesHandle,
        &GlobalTransform,
    )>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        reactions.clear();
        return;
    };
    let Some(species) = active_species.get(&species_assets) else {
        reactions.clear();
        return;
    };
    for reaction in reactions.read() {
        let Ok((
            viewing,
            phase,
            world,
            archetype,
            favourite,
            mut observation,
            mut meters,
            position,
        )) = guests.get_mut(reaction.guest)
        else {
            continue;
        };
        if *phase != GuestPhase::Visiting
            || reaction.viewing != *viewing
            || viewing.subject != reaction.subject
            || viewing.area.is_none()
            || observation
                .0
                .is_some_and(|observation| observation.viewing == *viewing)
        {
            continue;
        }
        let Some((guest_definition, guest_policy, viewing_policy)) = definitions
            .find_guest(archetype.0)
            .and_then(|guest| Some((guest, guest.donation?, definitions.guest_viewing()?)))
        else {
            continue;
        };
        let Ok((opportunity, subject_world, handle, subject_position)) =
            opportunities.get(reaction.subject)
        else {
            continue;
        };
        if subject_world.root != world.root {
            continue;
        }
        let Some(animal_policy) = species
            .find(handle.species)
            .and_then(|species| species.donation)
        else {
            continue;
        };
        let factor = if favourite.0 == Some(handle.species) {
            guest_policy.preferred_animal_factor
        } else {
            1.0
        };
        let distance_factor =
            openzt2_zoo_state::guests::guest_simulation_calculations::animal_view_distance_factor(
                guest_definition.view_scoring,
                position
                    .translation()
                    .distance(subject_position.translation()),
                viewing_policy.ground_range_m,
            );
        let meter = meters.0.entry(opportunity.category).or_default();
        // Signed authored viewing scores accumulate per species. The preferred
        // animal factor scales the score; a threshold funds one donation attempt.
        let Some(amount) = advance_species_donation_meter(
            meter,
            reaction.donation_score * distance_factor,
            factor,
            guest_policy.threshold,
            animal_policy.maximum_species_factor,
            opportunity.amount,
        ) else {
            continue;
        };
        observation.0 = Some(DonationObservation {
            viewing: *viewing,
            amount,
            acceptor_radius_cm: (guest_policy.search_radius_m * 100.0).min(u32::MAX as f32) as u32,
        });
    }
}

fn advance_species_donation_meter(
    meter: &mut f32,
    score: f32,
    preferred_factor: f32,
    threshold: f32,
    maximum_species_factor: f32,
    base_amount: Money,
) -> Option<Money> {
    if ![score, preferred_factor, threshold, maximum_species_factor]
        .into_iter()
        .all(f32::is_finite)
        || threshold <= 0.0
        || preferred_factor <= 0.0
        || maximum_species_factor < 0.0
    {
        return None;
    }
    let next = *meter + score * preferred_factor;
    if !next.is_finite() {
        return None;
    }
    *meter = next;
    if *meter < threshold {
        return None;
    }
    let amount_factor = (*meter / threshold).min(maximum_species_factor);
    let amount = (base_amount.0 as f64 * f64::from(amount_factor)).round();
    if !amount.is_finite() || amount <= 0.0 || amount >= i64::MAX as f64 {
        return None;
    }
    *meter -= threshold;
    Some(Money(amount as i64))
}

#[cfg(test)]
mod tests;

#[derive(Component, Default)]
pub(super) struct PendingDonationObservation(pub(super) Option<DonationObservation>);

pub(super) fn clear_donation_observations_for_ended_views(
    mut removed_viewing_components: RemovedComponents<Viewing>,
    changed_views: Query<Entity, Changed<Viewing>>,
    mut guests: Query<(Option<&Viewing>, &mut PendingDonationObservation), With<Guest>>,
) {
    for guest in removed_viewing_components
        .read()
        .chain(changed_views.iter())
    {
        let Ok((viewing, mut observation)) = guests.get_mut(guest) else {
            continue;
        };
        if observation
            .0
            .is_some_and(|observation| Some(&observation.viewing) != viewing)
        {
            observation.0 = None;
        }
    }
}

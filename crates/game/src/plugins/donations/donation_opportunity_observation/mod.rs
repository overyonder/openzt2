use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;

use crate::plugins::{
    guests::guest_simulation_types::{Guest, GuestPhase, GuestReactionApplied, Viewing},
    world_spawn::world_membership_types::WorldMember,
};

use super::{
    donation_guest_decision_calculations::create_donation_observation_from_authored_opportunity,
    donation_guest_decision_types::DonationObservation,
    donation_opportunity_types::DonationOpportunity,
};

pub(super) fn observe_donation_opportunities_after_applied_viewing_reactions(
    mut commands: Commands,
    mut reactions: MessageReader<GuestReactionApplied>,
    guests: Query<
        (
            &Viewing,
            &GuestPhase,
            &WorldMember,
            Option<&DonationObservation>,
        ),
        With<Guest>,
    >,
    opportunities: Query<(&DonationOpportunity, &WorldMember)>,
) {
    for reaction in reactions.read() {
        if reaction.kind != GuestMemoryKind::AnimalView {
            continue;
        }
        let Ok((viewing, phase, world, observation)) = guests.get(reaction.guest) else {
            continue;
        };
        if *phase != GuestPhase::Visiting
            || reaction.viewing != Some(*viewing)
            || viewing.subject != reaction.subject
            || viewing.area.is_none()
            || observation.is_some_and(|observation| observation.viewing == *viewing)
        {
            continue;
        }
        let Ok((opportunity, subject_world)) = opportunities.get(reaction.subject) else {
            continue;
        };
        if subject_world.root != world.root {
            continue;
        }
        // A policy is required independently of the reaction. Viewing scores
        // and elapsed viewing time do not define a currency amount or odds.
        commands.entity(reaction.guest).insert(
            create_donation_observation_from_authored_opportunity(*viewing, *opportunity),
        );
    }
}

pub(super) fn remove_guest_donation_observation_after_viewing_ends(
    mut commands: Commands,
    mut removed_viewing_components: RemovedComponents<Viewing>,
    guests: Query<(), With<Guest>>,
) {
    for guest in removed_viewing_components.read() {
        if guests.get(guest).is_ok() {
            commands.entity(guest).remove::<DonationObservation>();
        }
    }
}

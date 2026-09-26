use bevy::prelude::*;

use crate::plugins::guests::guest_simulation_types::{Guest, Viewing};

use super::{
    donation_guest_decision_calculations::create_donation_observation_from_authored_opportunity,
    donation_guest_decision_types::DonationObservation,
    donation_opportunity_types::DonationOpportunity,
};

pub(super) fn update_guest_donation_observation_after_viewed_subject_changes(
    mut commands: Commands,
    guests: Query<
        (Entity, &Viewing, Option<&DonationObservation>),
        (With<Guest>, Changed<Viewing>),
    >,
    opportunities: Query<&DonationOpportunity>,
) {
    for (guest, viewing, current_observation) in &guests {
        if current_observation.is_some_and(|observation| observation.subject == viewing.subject) {
            continue;
        }

        let Ok(opportunity) = opportunities.get(viewing.subject) else {
            commands.entity(guest).remove::<DonationObservation>();
            continue;
        };

        commands
            .entity(guest)
            .insert(create_donation_observation_from_authored_opportunity(
                viewing.subject,
                *opportunity,
            ));
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

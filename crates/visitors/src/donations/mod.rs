//! Guest donation decisions and exactly-once economy transfers.

mod donation_candidate_selection;
mod donation_guest_decision_calculations;
mod donation_guest_decision_types;
mod donation_interaction_progression;
mod donation_opportunity_observation;
use openzt2_zoo_state::donations::donation_opportunity_types;
mod donation_payment_calculations;
mod donation_payment_settlement;
mod donation_payment_submission;
use openzt2_zoo_state::donations::donation_payment_types;
mod donation_state_hydration;
mod zoo_donation_summary_presentation;

use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;

use self::{
    donation_candidate_selection::choose_donation_candidates_from_new_guest_observations,
    donation_interaction_progression::{
        advance_donation_interactions_and_request_completed_payments,
        begin_donation_interactions_after_guests_reach_acceptors,
        cancel_donation_interactions_with_missing_subjects_or_acceptors,
        reject_donation_candidates_after_navigation_failure,
    },
    donation_opportunity_observation::{
        hydrate_animal_donation_opportunities,
        observe_donation_opportunities_after_applied_viewing_reactions,
        remove_guest_donation_observation_after_viewing_ends,
    },
    donation_payment_settlement::{
        settle_completed_donation_transactions_and_update_acceptor_totals,
        settle_rejected_donation_transactions_and_clear_guest_payment_state,
    },
    donation_payment_submission::{
        claim_authorized_direct_donation_requests_before_transfer_submission,
        submit_validated_donation_requests_to_economy_transfers,
    },
    donation_payment_types::{DonationCompleted, DonationRejected, DonationRequest},
    donation_state_hydration::{
        hydrate_authored_donation_acceptors_and_docking_points,
        initialize_new_donation_acceptors_with_operating_age_and_profit,
    },
    zoo_donation_summary_presentation::project_zoo_donation_category_totals_into_authored_summary,
};

pub struct GuestDonationDecisionAndPaymentPlugin;

impl Plugin for GuestDonationDecisionAndPaymentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DonationRequest>()
            .add_message::<DonationCompleted>()
            .add_message::<DonationRejected>()
            .add_systems(
                Update,
                project_zoo_donation_category_totals_into_authored_summary,
            )
            .add_systems(
                Update,
                (
                    hydrate_authored_donation_acceptors_and_docking_points,
                    initialize_new_donation_acceptors_with_operating_age_and_profit,
                )
                    .chain()
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent),
            )
            .add_systems(
                FixedUpdate,
                (
                    hydrate_animal_donation_opportunities,
                    choose_donation_candidates_from_new_guest_observations,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                observe_donation_opportunities_after_applied_viewing_reactions
                    .after(
                        crate::guests::guest_lifecycle_and_memory_execution::apply_guest_reactions,
                    )
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    begin_donation_interactions_after_guests_reach_acceptors,
                    advance_donation_interactions_and_request_completed_payments,
                )
                    .chain()
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    claim_authorized_direct_donation_requests_before_transfer_submission,
                    submit_validated_donation_requests_to_economy_transfers,
                )
                    .chain()
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                (
                    settle_completed_donation_transactions_and_update_acceptor_totals,
                    settle_rejected_donation_transactions_and_clear_guest_payment_state,
                    reject_donation_candidates_after_navigation_failure,
                    cancel_donation_interactions_with_missing_subjects_or_acceptors,
                    remove_guest_donation_observation_after_viewing_ends,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}

#[cfg(any(test, feature = "testing"))]
pub fn add_donation_payment_test_systems(app: &mut App) {
    app.add_message::<openzt2_zoo_state::locomotion::locomotion_types::Arrived>()
        .add_message::<DonationRequest>()
        .add_message::<DonationCompleted>()
        .add_message::<DonationRejected>()
        .add_systems(
            Update,
            (
                hydrate_authored_donation_acceptors_and_docking_points,
                initialize_new_donation_acceptors_with_operating_age_and_profit,
                begin_donation_interactions_after_guests_reach_acceptors,
                advance_donation_interactions_and_request_completed_payments,
                claim_authorized_direct_donation_requests_before_transfer_submission,
                submit_validated_donation_requests_to_economy_transfers,
            )
                .chain()
                .in_set(FixedGameSet::Think),
        )
        .add_systems(
            Update,
            (
                settle_completed_donation_transactions_and_update_acceptor_totals,
                settle_rejected_donation_transactions_and_clear_guest_payment_state,
            )
                .chain()
                .in_set(FixedGameSet::Cleanup),
        );
}

#[cfg(any(test, feature = "testing"))]
pub fn queue_arrived_box_donation_for_test(
    world: &mut World,
    guest: Entity,
    acceptor: Entity,
    subject: Entity,
    amount: openzt2_zoo_state::economy::money_types::Money,
) {
    world
        .entity_mut(guest)
        .insert(donation_guest_decision_types::DonationCandidate {
            navigation_request_id: 1,
            acceptor,
            subject,
            amount,
        });
    world.write_message(openzt2_zoo_state::locomotion::locomotion_types::Arrived {
        entity: guest,
        request_id: 1,
        target: Some(acceptor),
    });
}

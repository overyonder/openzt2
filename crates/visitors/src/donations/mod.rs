//! Guest donation decisions and exactly-once economy transfers.

pub(crate) mod authored_view_donation_tokens;
mod donation_guest_decision_calculations;
mod donation_interaction_progression;
pub(crate) mod donation_opportunity_observation;
use openzt2_zoo_state::donations::donation_opportunity_types;
mod donation_payment_calculations;
mod donation_payment_settlement;
mod donation_payment_submission;
use openzt2_zoo_state::donations::donation_payment_types;
mod donation_state_hydration;
mod zoo_donation_summary_presentation;

use bevy::prelude::*;
use openzt2_zoo_state::donations::donation_guest_decision_types::SpeciesDonationCandidateLifetimes;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use self::{
    donation_interaction_progression::{
        advance_donation_interactions_and_request_completed_payments,
        begin_donation_interactions_after_guests_reach_acceptors,
        reconcile_donation_phase_relationships,
        reject_donation_candidates_after_navigation_failure,
    },
    donation_opportunity_observation::accumulate_signed_species_donation_scores,
    donation_payment_settlement::{
        settle_completed_donation_transactions_and_update_acceptor_totals,
        settle_rejected_donation_transactions_and_clear_guest_payment_state,
    },
    donation_payment_submission::{
        claim_authorized_direct_donation_requests_before_transfer_submission,
        submit_validated_donation_requests_to_economy_transfers,
    },
    donation_payment_types::{
        DonationCompleted, DonationRejected, DonationRequest, ZooDonationSummary,
    },
    donation_state_hydration::{
        initialize_new_donation_acceptors_with_operating_age_and_profit,
        project_donation_acceptors_and_docking_points,
    },
    zoo_donation_summary_presentation::{
        add_booked_donations, project_zoo_donation_summary, reconcile_donation_contributors,
        remove_booked_donations, withdraw_disabled_donations,
    },
};

pub struct GuestDonationDecisionAndPaymentPlugin;

impl Plugin for GuestDonationDecisionAndPaymentPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<openzt2_zoo_state::guests::guest_simulation_types::Guest, SpeciesDonationCandidateLifetimes>()
            .add_observer(donation_payment_submission::submit_authored_tour_donation)
            .add_message::<DonationRequest>()
            .add_message::<DonationCompleted>()
            .add_message::<DonationRejected>()
            .register_required_components::<WorldRoot, ZooDonationSummary>()
            .add_observer(add_booked_donations)
            .add_observer(remove_booked_donations)
            .add_observer(withdraw_disabled_donations)
            .add_systems(
                Update,
                (
                    reconcile_donation_contributors,
                    project_zoo_donation_summary.after(reconcile_donation_contributors),
                ),
            )
            .add_systems(
                FixedUpdate,
                reconcile_donation_contributors
                    .after(settle_completed_donation_transactions_and_update_acceptor_totals)
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                Update,
                (
                    project_donation_acceptors_and_docking_points,
                    initialize_new_donation_acceptors_with_operating_age_and_profit,
                )
                    .chain()
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent),
            )
            .add_systems(
                FixedUpdate,
                donation_opportunity_observation::decay_live_species_donation_candidates
                    .in_set(FixedGameSet::Clock),
            )
            .add_systems(
                FixedUpdate,
                accumulate_signed_species_donation_scores
                    .after(
                        crate::guests::view_event_execution::apply_guest_reactions,
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
                    submit_validated_donation_requests_to_economy_transfers
                        .in_set(openzt2_zoo_state::economy::SubmitSimulationAccountRequests),
                )
                    .chain()
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                (
                    donation_payment_settlement::resume_tour_donation_after_payment,
                    settle_completed_donation_transactions_and_update_acceptor_totals
                        .after_ignore_deferred(donation_payment_settlement::resume_tour_donation_after_payment),
                    settle_rejected_donation_transactions_and_clear_guest_payment_state
                        .after_ignore_deferred(donation_payment_settlement::resume_tour_donation_after_payment)
                        .after_ignore_deferred(settle_completed_donation_transactions_and_update_acceptor_totals),
                    reject_donation_candidates_after_navigation_failure
                        .after(settle_completed_donation_transactions_and_update_acceptor_totals)
                        .after(settle_rejected_donation_transactions_and_clear_guest_payment_state),
                    reconcile_donation_phase_relationships
                        .after(reject_donation_candidates_after_navigation_failure),
                )
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
                project_donation_acceptors_and_docking_points,
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
    world.entity_mut(guest).insert(
        openzt2_zoo_state::donations::donation_guest_decision_types::DonationCandidate {
            navigation_request_id: 1,
            acceptor,
            subject,
            amount,
        },
    );
    world.write_message(openzt2_zoo_state::locomotion::locomotion_types::Arrived {
        entity: guest,
        request_id: 1,
        target: Some(acceptor),
    });
}

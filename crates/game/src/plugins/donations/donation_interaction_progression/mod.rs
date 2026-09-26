use bevy::prelude::*;

use crate::plugins::{
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{Arrived, NavigationFailed},
};

use super::{
    donation_guest_decision_calculations::{
        begin_donation_progress_from_candidate, find_invalid_donation_relation_rejection,
    },
    donation_guest_decision_types::{DonationCandidate, DonationProgress},
    donation_opportunity_types::{DonationAcceptor, DonationOpportunity},
    donation_payment_types::{
        DonationPaymentRequested, DonationRejected, DonationRejection, DonationRequest,
    },
};

pub(super) fn begin_donation_interactions_after_guests_reach_acceptors(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    guests: Query<&DonationCandidate, With<Guest>>,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<&DonationOpportunity>,
) {
    for arrival in arrivals.read() {
        let Ok(candidate) = guests.get(arrival.entity) else {
            continue;
        };
        if arrival.request_id != candidate.navigation_request_id
            || arrival.target != Some(candidate.acceptor)
            || acceptors.get(candidate.acceptor).is_err()
        {
            continue;
        }
        let Ok(opportunity) = opportunities.get(candidate.subject) else {
            continue;
        };

        commands
            .entity(arrival.entity)
            .remove::<DonationCandidate>()
            .insert(begin_donation_progress_from_candidate(
                *candidate,
                opportunity.interaction_ticks,
            ));
    }
}

pub(super) fn advance_donation_interactions_and_request_completed_payments(
    mut commands: Commands,
    mut guests: Query<
        (Entity, &mut DonationProgress, &Wallet),
        (With<Guest>, Without<DonationPaymentRequested>),
    >,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<(), With<DonationOpportunity>>,
    mut payment_requests: MessageWriter<DonationRequest>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for (guest, mut progress, wallet) in &mut guests {
        let rejection = find_invalid_donation_relation_rejection(
            acceptors.get(progress.acceptor).is_ok(),
            opportunities.get(progress.subject).is_ok(),
        )
        .or_else(|| {
            (wallet.0 .0 < progress.amount.0).then_some(DonationRejection::InsufficientFunds)
        });
        if let Some(reason) = rejection {
            rejected_donations.write(DonationRejected {
                guest,
                acceptor: progress.acceptor,
                reason,
            });
            commands.entity(guest).remove::<DonationProgress>();
            continue;
        }
        if progress.remaining_ticks > 0 {
            progress.remaining_ticks -= 1;
            continue;
        }

        payment_requests.write(DonationRequest {
            guest,
            acceptor: progress.acceptor,
            subject: progress.subject,
            amount: progress.amount,
        });
        commands
            .entity(guest)
            .insert(DonationPaymentRequested { submitted: false });
    }
}

pub(super) fn reject_donation_candidates_after_navigation_failure(
    mut commands: Commands,
    mut navigation_failures: MessageReader<NavigationFailed>,
    guests: Query<&DonationCandidate, With<Guest>>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for failure in navigation_failures.read() {
        let Ok(candidate) = guests.get(failure.entity) else {
            continue;
        };

        if failure.request_id != candidate.navigation_request_id {
            continue;
        }

        rejected_donations.write(DonationRejected {
            guest: failure.entity,
            acceptor: candidate.acceptor,
            reason: DonationRejection::Unreachable,
        });
        commands
            .entity(failure.entity)
            .remove::<DonationCandidate>();
    }
}

pub(super) fn cancel_donation_interactions_with_missing_subjects_or_acceptors(
    mut commands: Commands,
    guests: Query<
        (
            Entity,
            Option<&DonationCandidate>,
            Option<&DonationProgress>,
        ),
        With<Guest>,
    >,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<(), With<DonationOpportunity>>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for (guest, candidate, progress) in &guests {
        let Some((acceptor, subject)) = candidate
            .map(|candidate| (candidate.acceptor, candidate.subject))
            .or_else(|| progress.map(|progress| (progress.acceptor, progress.subject)))
        else {
            continue;
        };

        let rejection = find_invalid_donation_relation_rejection(
            acceptors.get(acceptor).is_ok(),
            opportunities.get(subject).is_ok(),
        );
        if let Some(reason) = rejection {
            rejected_donations.write(DonationRejected {
                guest,
                acceptor,
                reason,
            });
            commands
                .entity(guest)
                .remove::<DonationCandidate>()
                .remove::<DonationProgress>()
                .remove::<DonationPaymentRequested>();
        }
    }
}

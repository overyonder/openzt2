use bevy::prelude::Entity;

use openzt2_zoo_state::economy::money_types::Money;

use super::{
    donation_guest_decision_types::{DonationCandidate, DonationObservation, DonationProgress},
    donation_payment_types::DonationRejection,
};

pub(super) fn calculate_eligible_donation_amount(
    wallet: Money,
    observation: DonationObservation,
) -> Option<Money> {
    (observation.amount.0 > 0 && wallet.0 >= observation.amount.0).then_some(observation.amount)
}

pub(super) fn create_donation_candidate_from_observation(
    observation: DonationObservation,
    acceptor: Entity,
    amount: Money,
    navigation_request_id: u64,
) -> DonationCandidate {
    DonationCandidate {
        navigation_request_id,
        acceptor,
        subject: observation.viewing.subject,
        amount,
    }
}

pub(super) fn begin_donation_progress_from_candidate(
    candidate: DonationCandidate,
    interaction_ticks: u32,
) -> DonationProgress {
    DonationProgress {
        acceptor: candidate.acceptor,
        subject: candidate.subject,
        amount: candidate.amount,
        remaining_ticks: interaction_ticks,
    }
}

pub(super) const fn find_invalid_donation_relation_rejection(
    acceptor_exists: bool,
    subject_exists: bool,
) -> Option<DonationRejection> {
    if !acceptor_exists {
        Some(DonationRejection::InvalidAcceptor)
    } else if !subject_exists {
        Some(DonationRejection::InvalidSubject)
    } else {
        None
    }
}

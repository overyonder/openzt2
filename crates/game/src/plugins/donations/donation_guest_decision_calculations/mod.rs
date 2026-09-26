use bevy::prelude::Entity;

use crate::plugins::economy::money_types::Money;

use super::{
    donation_guest_decision_types::{DonationCandidate, DonationObservation, DonationProgress},
    donation_opportunity_types::DonationOpportunity,
    donation_payment_types::DonationRejection,
};

pub(super) fn calculate_eligible_donation_amount(
    satisfaction_permille: u16,
    wallet: Money,
    observation: DonationObservation,
    probability_roll: Option<u32>,
) -> Option<Money> {
    if observation.amount.0 <= 0
        || wallet.0 < observation.amount.0
        || satisfaction_permille < observation.minimum_satisfaction_permille
        || observation.chance_permyriad > 10_000
    {
        return None;
    }

    let chance_permyriad = u32::from(observation.chance_permyriad);
    (chance_permyriad == 10_000 || probability_roll.is_some_and(|roll| roll < chance_permyriad))
        .then_some(observation.amount)
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
        subject: observation.subject,
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

pub(super) fn create_donation_observation_from_authored_opportunity(
    subject: Entity,
    opportunity: DonationOpportunity,
) -> DonationObservation {
    DonationObservation {
        subject,
        amount: opportunity.amount,
        minimum_satisfaction_permille: opportunity.minimum_satisfaction_permille,
        chance_permyriad: opportunity.chance_permyriad,
        acceptor_radius_cells: opportunity.acceptor_radius_cells,
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

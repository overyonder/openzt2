use openzt2_zoo_state::donations::donation_guest_decision_types::{
    DonationCandidate, DonationProgress,
};

use super::donation_payment_types::DonationRejection;

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

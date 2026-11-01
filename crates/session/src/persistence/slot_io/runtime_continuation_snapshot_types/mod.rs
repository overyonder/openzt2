use openzt2_zoo_state::guests::guest_simulation_types::{GuestArrivalState, GuestSurvey};

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeContinuationSnapshot {
    pub next_persistent_identifier: u64,
    /// Last issued identity, including identities whose owners no longer exist.
    pub navigation_sequence: u64,
    /// Last issued root execution identity.
    pub behavior_execution_sequence: u64,
    pub service_request_sequence: Option<u64>,
    pub guest_arrivals: Option<GuestArrivalState>,
    pub guest_survey: Option<GuestSurvey>,
}

/// Maxima include every restored actor, buffered request, result and stable reference.
#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeContinuationIdentityHighWaterMarks<'a> {
    pub maximum_persistent_identifier: u64,
    pub maximum_navigation_request: u64,
    pub maximum_behavior_execution: u64,
    /// One entry per pending request owner, rather than per reference to that owner.
    pub pending_service_request_sequences: &'a [u64],
}

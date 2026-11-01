use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalState;
use openzt2_zoo_state::guests::guest_survey_types::GuestSurvey;

#[derive(Debug, Clone)]
pub(super) struct RuntimeContinuationSnapshot {
    pub general_economy:
        Option<openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger>,
    pub entity_names: Option<Vec<(u64, String)>>,
    pub world_audio_preset: Option<openzt2_game_data::audio::AudioDocument>,
    pub entity_name_counters: Option<openzt2_zoo_state::entity_naming_types::EntityNameTagCounters>,
    pub registered_entity_name_identifiers: Option<Vec<u64>>,
    pub next_persistent_identifier: u64,
    /// Last issued identity, including identities whose owners no longer exist.
    pub navigation_sequence: u64,
    /// Last issued root execution identity.
    pub behavior_execution_sequence: u64,
    pub service_request_sequence: Option<u64>,
    pub guest_arrivals: Option<GuestArrivalState>,
    pub guest_survey: Option<GuestSurvey>,
    pub guest_favourite_animal_trend: Option<GuestFavouriteAnimalTrend>,
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

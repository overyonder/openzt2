use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalState;
use openzt2_zoo_state::guests::guest_survey_types::{GuestSurvey, GuestSurveyNeedCadence};

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
    pub guest_survey: Option<(GuestSurveyNeedCadence, GuestSurvey)>,
    pub guest_favourite_animal_trend: Option<GuestFavouriteAnimalTrend>,
    pub domain_continuation: Option<RuntimeDomainContinuation>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct RuntimeDomainContinuation {
    pub disease: DiseaseContinuation,
    pub eggs: Vec<EggContinuation>,
    pub fossils: Option<super::fossil_continuation_snapshot::FossilContinuation>,
    #[serde(skip)]
    pub breeding_center_births: Vec<(u64, f32)>,
    #[serde(skip)]
    pub water_filter_cleanup: Vec<(u64, u64)>,
    #[serde(skip)]
    pub guests: Vec<super::guest_continuation_snapshot::GuestContinuation>,
    pub trick_popularity: Option<std::collections::BTreeMap<openzt2_game_data::AssetId, f32>>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct DiseaseContinuation {
    pub funding: Option<f32>,
    pub exposure_deadline: Option<u64>,
    pub active: Option<(openzt2_game_data::AssetId, u64)>,
    #[serde(skip)]
    pub assignments: Vec<(u64, openzt2_game_data::AssetId)>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub(super) struct EggContinuation {
    pub id: u64,
    pub amount: f32,
    pub remaining_ns: u64,
    pub random: [u64; 2],
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

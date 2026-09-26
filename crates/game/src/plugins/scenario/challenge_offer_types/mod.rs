use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioChallengeOfferState {
    Offered,
    Active,
    Declined,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioChallengeOffer {
    pub definition: AssetId,
    pub expires_tick: u64,
    pub state: ScenarioChallengeOfferState,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ScenarioChallengeSelectionRandomNumberGenerator(DeterministicRng);

impl ScenarioChallengeSelectionRandomNumberGenerator {
    pub(crate) fn create_from_zoo_seed(
        seed: crate::plugins::simulation_time::deterministic_random_stream::ZooSeed,
    ) -> Self {
        Self(DeterministicRng::from_entity(
            seed,
            crate::plugins::world_spawn::persistent_id_types::PersistentId(1),
            crate::plugins::simulation_time::deterministic_random_stream::RngDomain::Challenge,
        ))
    }

    pub(crate) fn generate_next_challenge_selection_value(&mut self) -> u32 {
        self.0.next_u32()
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioChallengeLuaValidationDay(pub u32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RespondToScenarioChallengeRequest {
    pub offer: Entity,
    pub accept: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioChallengeAccepted {
    pub offer: Entity,
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateScenarioChallengeOfferFromLuaSourcePathRequest {
    pub source_path: String,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioChallengeAcceptanceRejected {
    pub(crate) offer: Entity,
    pub(crate) reason: ScenarioChallengeAcceptanceRejectionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScenarioChallengeAcceptanceRejectionReason {
    Missing,
    Expired,
    AlreadyActive,
    Ineligible,
}

/// Native UI operations emitted while a challenge's authored Lua runs.
#[derive(Message, Debug, Clone, Copy)]
pub(super) struct ScenarioChallengePanelRequest {
    pub(super) scenario: AssetId,
    pub(super) show: bool,
    pub(super) text: Option<AssetId>,
}

/// The localization key currently displayed by this offer's authored dialog.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ScenarioChallengePanelText(pub(super) AssetId);

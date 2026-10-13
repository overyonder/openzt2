use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

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
        seed: openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed,
    ) -> Self {
        Self(DeterministicRng::from_entity(
            seed,
            openzt2_world_state::world_spawn::persistent_id_types::PersistentId(1),
            openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain::Challenge,
        ))
    }

    pub(crate) fn generate_next_challenge_selection_value(&mut self) -> u32 {
        self.0.next_u32()
    }
}

/// The zoo tick of this world's next challenge-manager evaluation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioChallengeManagerNextEvaluationTick(pub u64);

/// This world has had its one chance at an instant challenge.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioInstantChallengeDecided;

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

/// The authored modal in `ui/layout/challenge.xml` that offers a challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScenarioChallengePanelSurface {
    /// `challenge layout`, opened by `showchallengepanel`.
    Offer,
    /// `instant challenge layout`, opened by `showinstantchallengepanel`.
    InstantOffer,
}

/// Native UI operations emitted while a challenge's authored Lua runs.
#[derive(Message, Debug, Clone, Copy)]
pub(super) struct ScenarioChallengePanelRequest {
    pub(super) scenario: AssetId,
    /// The offer surface the script showed, if it showed one.
    pub(super) show: Option<ScenarioChallengePanelSurface>,
    /// Key set on `challenge text` or `instant challenge text`.
    pub(super) text: Option<AssetId>,
    /// Key set on `instant challenge description text`.
    pub(super) description: Option<AssetId>,
}

/// The localization key currently displayed as this offer's challenge text.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ScenarioChallengePanelText(pub(super) AssetId);

/// The localization key currently displayed as this offer's instant-challenge
/// description.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ScenarioChallengePanelDescription(pub(super) AssetId);

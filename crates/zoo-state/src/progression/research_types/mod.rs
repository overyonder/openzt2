use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectAvailability {
    pub item: AssetId,
    pub available: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchAvailabilityUnlockCountdown {
    pub item: AssetId,
    pub remaining_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProject {
    pub definition: AssetId,
    pub elapsed_ticks: u64,
    pub required_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResearchProjectPaused;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingResearchProjectPayment {
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartResearchProjectRequest {
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectCompleted {
    pub project: Entity,
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectCompletionReady {
    pub project: Entity,
    pub definition: AssetId,
}

/// Overrides how long one research project takes, from a scenario script.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SetResearchDuration {
    pub definition: AssetId,
    pub seconds: f32,
}

/// Scenario-set research durations, in seconds of simulation time.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct ResearchDurationOverrides(pub std::collections::BTreeMap<AssetId, f32>);

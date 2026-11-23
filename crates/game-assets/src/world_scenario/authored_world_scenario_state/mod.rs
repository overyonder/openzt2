use std::collections::BTreeMap;

use openzt2_game_data::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScenarioNodePolicy {
    pub initial_state: Option<i32>,
    pub sticky: Option<bool>,
    pub optional: Option<bool>,
    pub chain: bool,
    pub update_when_paused: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AuthoredWorldScenarioState {
    pub scenario_nodes: BTreeMap<AssetId, ScenarioNodePolicy>,
    pub former_fence_types: BTreeMap<(AssetId, u64), AssetId>,
}

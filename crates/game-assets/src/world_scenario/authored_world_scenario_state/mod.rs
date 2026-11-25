use std::{collections::BTreeMap, io};

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
    #[serde(skip)]
    pub(super) legacy_script_references: bool,
}

impl AuthoredWorldScenarioState {
    pub fn merge(&mut self, additional: Self) -> io::Result<()> {
        if additional.scenario_nodes.iter().any(|(key, policy)| {
            self.scenario_nodes
                .get(key)
                .is_some_and(|existing| existing != policy)
        }) || additional.former_fence_types.iter().any(|(key, former)| {
            self.former_fence_types
                .get(key)
                .is_some_and(|existing| existing != former)
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "conflicting authored world state",
            ));
        }
        self.scenario_nodes.extend(additional.scenario_nodes);
        self.former_fence_types
            .extend(additional.former_fence_types);
        self.legacy_script_references |= additional.legacy_script_references;
        Ok(())
    }
}

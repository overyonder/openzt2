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

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AuthoredScenarioUiState {
    pub visible: Option<bool>,
    pub active: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AuthoredScenarioUiModification {
    pub document: Option<AssetId>,
    pub node: String,
    pub old: AuthoredScenarioUiState,
    pub new: AuthoredScenarioUiState,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AuthoredWorldScenarioState {
    pub scenario_nodes: BTreeMap<AssetId, ScenarioNodePolicy>,
    pub former_fence_types: BTreeMap<(AssetId, u64), AssetId>,
    pub objective_roots: BTreeMap<AssetId, Vec<AssetId>>,
    pub ui_modifications: BTreeMap<AssetId, Vec<AuthoredScenarioUiModification>>,
    #[serde(skip)]
    pub(crate) original_saved_entity_handles: BTreeMap<u32, u64>,
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
        }) || additional.objective_roots.iter().any(|(key, roots)| {
            self.objective_roots
                .get(key)
                .is_some_and(|existing| existing != roots)
        }) || additional.ui_modifications.iter().any(|(key, rules)| {
            self.ui_modifications
                .get(key)
                .is_some_and(|existing| existing != rules)
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "conflicting authored world state",
            ));
        }
        self.scenario_nodes.extend(additional.scenario_nodes);
        self.former_fence_types
            .extend(additional.former_fence_types);
        self.objective_roots.extend(additional.objective_roots);
        self.ui_modifications.extend(additional.ui_modifications);
        self.legacy_script_references |= additional.legacy_script_references;
        Ok(())
    }

    pub(super) fn remap_scenario(&mut self, from: AssetId, to: AssetId) {
        if let Some(roots) = self.objective_roots.remove(&from) {
            self.objective_roots.insert(to, roots);
        }
        if let Some(rules) = self.ui_modifications.remove(&from) {
            self.ui_modifications.insert(to, rules);
        }
    }
}

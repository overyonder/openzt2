use serde::{Deserialize, Serialize};

pub(crate) const SCENARIO_GLOBAL_VARIABLES: &str = "__openzt2_scenario_state";

pub(crate) const SAVED_SCENARIO_ACTION_ARGUMENTS: &str = "__openzt2_action_arguments";

pub(crate) fn saved_scenario_action_key(
    binding: &openzt2_game_data::world_scenario::ScenarioScriptBinding,
) -> openzt2_game_data::AssetId {
    use openzt2_game_assets::source_document::path::AssetPath;
    use openzt2_game_data::world_scenario::ScenarioScriptPhase;

    let phase = match binding.phase {
        ScenarioScriptPhase::Validate => 0,
        ScenarioScriptPhase::Evaluate => 1,
        ScenarioScriptPhase::Success => 2,
        ScenarioScriptPhase::Failure => 3,
        ScenarioScriptPhase::AlwaysRun => 4,
    };
    let source = AssetPath::new(&binding.script).key();
    let mut hash = blake3::Hasher::new_derive_key("OpenZT2 saved scenario action");
    hash.update(&binding.objective.0);
    hash.update(&[phase]);
    hash.update(&(source.len() as u64).to_le_bytes());
    hash.update(source.as_bytes());
    hash.update(binding.entry.as_bytes());
    let mut id = [0; 16];
    hash.finalize_xof().fill(&mut id);
    openzt2_game_data::AssetId(id)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ScenarioLuaSavedValue {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Bytes(Vec<u8>),
    Table(Vec<(ScenarioLuaSavedValue, ScenarioLuaSavedValue)>),
    PersistentEntity(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioLuaSavedState {
    pub(crate) modules: Vec<String>,
    pub(crate) globals: Vec<(Vec<u8>, ScenarioLuaSavedValue)>,
}

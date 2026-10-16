use serde::{Deserialize, Serialize};

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

//! Authored research projects and catalogue unlock requirements.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ResearchDefinition {
    pub id: AssetId,
    pub name_key: AssetId,
    pub cost_cents: i64,
    pub duration: ResearchDuration,
    pub minimum_fame_percent: f32,
    pub prerequisites: Vec<AssetId>,
    pub unlocks: Vec<AssetId>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum UnlockRequirement {
    Always,
    Fame(u16),
    Research(AssetId),
    Scenario(AssetId),
    Award(AssetId),
    Profile(AssetId),
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct UnlockDefinition {
    pub id: AssetId,
    pub target: AssetId,
    pub requirement: UnlockRequirement,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum ResearchDuration {
    Ticks(u64),
    Nanoseconds(u64),
    RandomDefault,
}

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioObjective {
    pub scenario: AssetId,
    pub record_index: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioObjectiveStatus {
    Inactive,
    Active,
    Satisfied,
    Failed,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioObjectiveProgress {
    pub current: i64,
    pub target: i64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioObjectiveDeadline {
    pub end_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioObjectiveAuthoredVisibility(pub bool);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioObjectiveStatusChanged {
    pub objective: Entity,
    pub status: ScenarioObjectiveStatus,
    pub current: i64,
    pub target: i64,
}

#[derive(Component, Debug, Default)]
pub(crate) struct ScenarioObjectivePrerequisiteEntities(pub Box<[Entity]>);

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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ScenarioObjectives)]
pub struct ScenarioObjectiveOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioObjectiveOf, linked_spawn)]
pub struct ScenarioObjectives(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ScenarioObjectivePrerequisites)]
pub struct ScenarioObjectivePrerequisiteFor(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioObjectivePrerequisiteFor, linked_spawn)]
pub struct ScenarioObjectivePrerequisites(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ScenarioObjectiveDependents)]
pub struct ScenarioObjectiveRequires(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioObjectiveRequires, linked_spawn)]
pub struct ScenarioObjectiveDependents(Vec<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowScenarioRuleRequest {
    pub source: Option<Entity>,
    pub scenario: AssetId,
    pub rule: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioObjectiveGroup {
    pub record_index: usize,
    pub status: ScenarioObjectiveStatus,
}

#[derive(Component)]
#[relationship(relationship_target = ScenarioObjectiveGroups)]
pub struct ScenarioObjectiveGroupOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioObjectiveGroupOf, linked_spawn)]
pub struct ScenarioObjectiveGroups(Vec<Entity>);

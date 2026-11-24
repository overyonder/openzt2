use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorInstructionId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Component)]
#[relationship(relationship_target = StageShowAudience)]
#[require(GuestShowExperience)]
pub(super) struct GuestWatchingStage {
    #[relationship]
    pub(super) stage: Entity,
    pub(super) location: Option<Entity>,
    pub(super) instruction: Option<BehaviorInstructionId>,
}

#[derive(Component)]
#[relationship_target(relationship = GuestWatchingStage)]
pub(super) struct StageShowAudience(Vec<Entity>);

#[derive(Component, Default)]
pub(super) struct GuestShowExperience {
    pub(super) show: Option<Entity>,
    pub(super) saw_favourite: bool,
    pub(super) reactions: BTreeMap<AssetId, u32>,
    pub(super) delivered_events: BTreeSet<Entity>,
}

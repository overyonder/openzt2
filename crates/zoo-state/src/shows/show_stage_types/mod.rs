use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowStage {
    pub show_stage_definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowStageOpenState {
    Open,
    Closed,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ShowName(pub String);

/// A stage's containing habitat, irrespective of the tank's depth policy.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = HabitatShowStages)]
pub struct ShowStageInHabitat(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowStageInHabitat)]
pub struct HabitatShowStages(Vec<Entity>);

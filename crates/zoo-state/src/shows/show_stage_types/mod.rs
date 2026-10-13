use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowStage {
    pub show_stage_definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShowStageHydrated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowStageOpenState {
    Open,
    Closed,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ShowName(pub String);

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowStage {
    pub(crate) show_stage_definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShowStageHydrated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShowStageOpenState {
    Open,
    Closed,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShowName(pub(crate) String);

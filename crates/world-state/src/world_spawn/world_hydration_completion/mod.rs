use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldLoadFinished {
    pub operation: Option<Entity>,
    pub scenario: openzt2_game_data::AssetId,
    pub root: Entity,
}

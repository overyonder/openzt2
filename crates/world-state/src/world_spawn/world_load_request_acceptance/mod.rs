use bevy::prelude::Entity;
use openzt2_game_data::AssetId;

#[derive(bevy::prelude::Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginWorldLoad {
    pub operation: Option<Entity>,
    pub scenario: AssetId,
    pub mode: crate::game_session_types::WorldSessionMode,
    pub profile: AssetId,
    pub starting_cash_cents: Option<i64>,
}

#[derive(bevy::prelude::Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldLoadOperationOwner(pub Entity);

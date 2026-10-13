//! Request and pending state for starting an explicit animal behavior set.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartBehaviorSet {
    pub actor: Entity,
    pub program: AssetId,
    pub target: Option<Entity>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingBehaviorSet {
    pub program: AssetId,
    pub target: Option<Entity>,
}

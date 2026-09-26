//! Request and pending state for starting an explicit animal behavior set.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartBehaviorSet {
    pub(crate) actor: Entity,
    pub(crate) program: AssetId,
    pub(crate) target: Option<Entity>,
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PendingBehaviorSet {
    pub(super) program: AssetId,
    pub(super) target: Option<Entity>,
}

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// ChildOf names the recipient; a running task may claim this token as its object.
#[derive(Component)]
pub struct BehaviorSelfToken {
    pub name: AssetId,
    pub available_at: u64,
    pub expires_at: u64,
}

#[derive(Component)]
pub struct ClaimedBehaviorSelfToken {
    pub execution_id: u64,
}

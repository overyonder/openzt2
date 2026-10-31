use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// ChildOf names the recipient; a running task may claim this token as its object.
#[derive(Component)]
pub struct BehaviorSelfToken {
    pub name: AssetId,
    pub available_at: u64,
    pub expires_at: u64,
    /// The authored `Payload` participant, the target of a `fromToken` task.
    pub payload: Option<Entity>,
    /// Whether a failed claiming task leaves the token in place.
    pub retain_on_failure: bool,
}

#[derive(Component)]
pub struct ClaimedBehaviorSelfToken {
    pub execution_id: u64,
}

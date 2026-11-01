use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy)]
pub struct BehaviorSubjectTrigger {
    pub subject: Entity,
    pub trigger: AssetId,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BehaviorTriggerDelivery;

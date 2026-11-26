use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StaffRequestFactValue {
    Number(f32),
    Boolean(bool),
}

#[derive(Message, Clone, Copy, Debug)]
pub struct StaffRequestFactChanged {
    pub owner: Entity,
    pub key: AssetId,
    pub previous: StaffRequestFactValue,
    pub current: StaffRequestFactValue,
}

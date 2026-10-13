use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Clone, Copy)]
pub struct PhysicalPresentationRequest {
    pub owner: Entity,
    pub operation: PhysicalPresentationOperation,
}

#[derive(Clone, Copy)]
pub enum PhysicalPresentationOperation {
    Set(AssetId),
    Push(AssetId),
    Completed(Entity),
    Looped(Entity),
}

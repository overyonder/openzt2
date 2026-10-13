use bevy::prelude::*;
use openzt2_game_data::world_definitions::environment::EnvironmentLightTarget;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentLight {
    pub keyframe: u16,
    /// Which part of the scene this light illuminates.
    pub target: EnvironmentLightTarget,
    /// Primitive rank of the authored light kind. Zero is the consumer-shaped
    /// single-light curve; legacy rigs use one, two, and three for sun, side,
    /// and back respectively.
    pub kind_rank: u8,
}

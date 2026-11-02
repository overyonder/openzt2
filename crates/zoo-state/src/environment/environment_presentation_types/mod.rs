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

/// Current authored specular contribution, independent of diffuse light colour.
#[derive(Component, Debug, Clone, Copy)]
pub struct EnvironmentSpecularLightColor(pub Color);

/// The authored terrain ambient term. Objects and terrain have separate light
/// programs, and Bevy's ambient light carries the objects'.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvironmentTerrainAmbientLight(pub Option<Vec3>);

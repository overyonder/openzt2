use bevy::prelude::Component;
use bevy::prelude::Vec2;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct WorldTerrainHorizontalBounds {
    pub min: Vec2,
    pub max: Vec2,
}

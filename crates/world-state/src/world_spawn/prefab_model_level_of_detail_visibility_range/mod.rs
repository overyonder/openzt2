use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PrefabModelLevelOfDetailVisibilityRange {
    pub group: Entity,
    pub ordinal: u16,
    pub center_m: Vec3,
    pub near_m: f32,
    pub far_m: f32,
    pub active_without_range: bool,
}

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrefabModelLevelOfDetailVisibilityRange {
    pub(crate) group: Entity,
    pub(crate) ordinal: u16,
    pub(crate) center_m: Vec3,
    pub(crate) near_m: f32,
    pub(crate) far_m: f32,
    pub(crate) active_without_range: bool,
}

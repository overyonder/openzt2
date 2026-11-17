use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[relationship(relationship_target = PrefabLodRanges, allow_self_referential)]
pub struct PrefabModelLevelOfDetailVisibilityRange {
    #[relationship]
    pub group: Entity,
    pub ordinal: u16,
    pub center_m: Vec3,
    pub near_m: f32,
    pub far_m: f32,
    pub active_without_range: bool,
}

#[derive(Component, Debug, Deref)]
#[relationship_target(relationship = PrefabModelLevelOfDetailVisibilityRange)]
pub struct PrefabLodRanges(Vec<Entity>);

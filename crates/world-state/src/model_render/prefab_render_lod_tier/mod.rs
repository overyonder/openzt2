use bevy::prelude::*;

use openzt2_game_data::scene_prefab::PrefabRenderLodPolicy;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabRenderLodTier(pub u16);

/// The live transform that supplies distance for this authored render group.
#[derive(Component, Debug, Clone)]
pub struct PrefabRenderLodDistance {
    pub origin: Entity,
    pub policy: PrefabRenderLodPolicy,
}

/// Actor or model overrides of the authored render group's distance thresholds.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct PrefabRenderLodDistanceOverrides {
    pub switch_distance_squared: Vec<f32>,
    pub minimum_switch_distance_squared: Vec<f32>,
}

#[derive(Component)]
#[relationship(relationship_target = PrefabRenderLodVariants, allow_self_referential)]
pub struct PrefabRenderLodVariant {
    #[relationship]
    pub group: Entity,
    pub ordinal: u16,
}

#[derive(Component)]
#[relationship_target(relationship = PrefabRenderLodVariant)]
pub struct PrefabRenderLodVariants(Vec<Entity>);

#[derive(Component, PartialEq)]
pub struct PrefabRenderLodSelection(pub bool);

use bevy::prelude::*;

#[derive(Component)]
pub struct PrefabRenderLodTier(pub u16);

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

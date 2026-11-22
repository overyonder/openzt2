use bevy::ecs::{lifecycle::HookContext, relationship::RelationshipTarget, world::DeferredWorld};
use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldEntityIsCrated;

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ContainedCratedAnimal)]
#[require(WorldEntityIsCrated, Visibility = Visibility::Hidden)]
#[component(on_add = hide_physically_crated_animal)]
pub struct AnimalCrate(pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = AnimalCrate)]
pub struct ContainedCratedAnimal(Entity);

impl ContainedCratedAnimal {
    pub fn occupant(&self) -> Entity {
        *self.collection()
    }
}

fn hide_physically_crated_animal(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut visibility) = world.get_mut::<Visibility>(context.entity) {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWorldEntityFromCrate(pub Entity);

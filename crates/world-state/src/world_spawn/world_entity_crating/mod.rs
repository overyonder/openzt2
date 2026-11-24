use bevy::ecs::{lifecycle::HookContext, relationship::RelationshipTarget, world::DeferredWorld};
use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ContainedCratedAnimal)]
#[require(Visibility = Visibility::Hidden)]
#[component(on_add = hide_crated_entity, on_remove = restore_uncrated_visibility)]
pub struct AnimalCrate(pub Entity);

/// Script-owned crating without a physical crate link.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[require(Visibility = Visibility::Hidden)]
#[component(on_add = hide_crated_entity, on_remove = restore_uncrated_visibility)]
pub struct ScriptedCrating;

#[derive(Component, Debug)]
#[relationship_target(relationship = AnimalCrate)]
pub struct ContainedCratedAnimal(Entity);

impl ContainedCratedAnimal {
    pub fn occupant(&self) -> Entity {
        *self.collection()
    }
}

fn hide_crated_entity(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut visibility) = world.get_mut::<Visibility>(context.entity) {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn restore_uncrated_visibility(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    // Removal hooks run while the old components can still be queried.
    world.commands().queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        if !entity.contains::<AnimalCrate>() && !entity.contains::<ScriptedCrating>() {
            if let Some(mut visibility) = entity.get_mut::<Visibility>() {
                visibility.set_if_neq(Visibility::Inherited);
            }
        }
    });
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWorldEntityFromCrate(pub Entity);

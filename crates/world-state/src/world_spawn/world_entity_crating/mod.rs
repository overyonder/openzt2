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

pub type CratedWorldEntity = Or<(With<AnimalCrate>, With<ScriptedCrating>)>;
pub type UncratedWorldEntity = (Without<AnimalCrate>, Without<ScriptedCrating>);

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
    let still_crated = if world.component_id::<AnimalCrate>() == Some(context.component_id) {
        world.get::<ScriptedCrating>(context.entity).is_some()
    } else {
        world.get::<AnimalCrate>(context.entity).is_some()
    };
    if !still_crated {
        if let Some(mut visibility) = world.get_mut::<Visibility>(context.entity) {
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWorldEntityFromCrate(pub Entity);

#[cfg(test)]
mod tests;

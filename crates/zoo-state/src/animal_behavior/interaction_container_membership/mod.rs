use bevy::ecs::{
    lifecycle::HookContext, relationship::RelationshipTarget, system::SystemParam,
    world::DeferredWorld,
};
use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectInteractionSlotDefinition, AssetId,
};

pub mod requests;
#[cfg(test)]
mod tests;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = InteractionContainerMembers, allow_self_referential)]
#[component(on_discard = clear_docking_on_membership_discard)]
pub struct InteractionContainerMember {
    #[relationship]
    pub container: Entity,
    pub slot: usize,
}

/// Bevy appends admissions and removes members without reordering the others.
#[derive(Component)]
#[relationship_target(relationship = InteractionContainerMember)]
pub struct InteractionContainerMembers(Vec<Entity>);

#[derive(Component)]
pub struct DockedAtInteractionContainer;

fn clear_docking_on_membership_discard(mut world: DeferredWorld, context: HookContext) {
    if world
        .get::<DockedAtInteractionContainer>(context.entity)
        .is_some()
    {
        world
            .commands()
            .entity(context.entity)
            .try_remove::<DockedAtInteractionContainer>();
    }
}

#[derive(SystemParam)]
pub struct InteractionContainerMemberships<'w, 's> {
    members: Query<
        'w,
        's,
        (
            &'static InteractionContainerMember,
            Has<DockedAtInteractionContainer>,
        ),
    >,
    containers: Query<'w, 's, &'static InteractionContainerMembers>,
}

impl<'w, 's> InteractionContainerMemberships<'w, 's> {
    fn membership_of(&self, member: Entity) -> Option<InteractionContainerMember> {
        self.members
            .get(member)
            .ok()
            .map(|(membership, _)| *membership)
    }

    #[must_use]
    pub fn container_for_member(&self, member: Entity) -> Option<(Entity, usize)> {
        self.membership_of(member)
            .map(|membership| (membership.container, membership.slot))
    }

    pub fn members_of_container<'a>(
        &'a self,
        container: Entity,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        self.containers
            .get(container)
            .into_iter()
            .flat_map(|members| members.iter())
    }

    pub fn members_in_slot<'a>(
        &'a self,
        container: Entity,
        slot: usize,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        self.members_of_container(container)
            .filter(move |member| self.actor_slot(container, *member) == Some(slot))
    }

    #[must_use]
    pub fn occupied_places(&self, target: Entity, slot: usize) -> usize {
        self.members_in_slot(target, slot).count()
    }

    #[must_use]
    pub fn actor_slot(&self, target: Entity, actor: Entity) -> Option<usize> {
        self.membership_of(actor)
            .filter(|membership| membership.container == target)
            .map(|membership| membership.slot)
    }

    pub fn docked_member_entities_in_container<'a>(
        &'a self,
        container: Entity,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        self.members_of_container(container)
            .filter(move |member| self.is_docked(container, *member))
    }

    #[must_use]
    pub fn is_docked(&self, target: Entity, actor: Entity) -> bool {
        self.members
            .get(actor)
            .is_ok_and(|(membership, docked)| docked && membership.container == target)
    }

    /// Reuse a service position when possible, otherwise try the nearest slots first.
    pub fn service_slot_candidates(
        &self,
        target: Entity,
        actor: Entity,
        slots: &[WorldObjectInteractionSlotDefinition],
        reservation_tag: Option<AssetId>,
        mut ground_offset: impl FnMut(&str) -> Option<Vec2>,
    ) -> Vec<(usize, u8)> {
        let existing = self.actor_slot(target, actor);
        let mut candidates = slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                !slot.is_queue
                    && slot.capacity != 0
                    && reservation_tag.is_none_or(|tag| slot.reservation_tag == tag)
            })
            .filter_map(|(index, slot)| {
                let distance = ground_offset(&slot.target_node_name)?.length_squared();
                Some((index, slot.capacity, distance))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable_by(|&(left, _, left_distance), &(right, _, right_distance)| {
            (existing != Some(left))
                .cmp(&(existing != Some(right)))
                .then_with(|| left_distance.total_cmp(&right_distance))
                .then_with(|| left.cmp(&right))
        });
        candidates
            .into_iter()
            .map(|(slot, capacity, _)| (slot, capacity))
            .collect()
    }

    #[must_use]
    pub fn queue_front_can_enter_service(
        &self,
        target: Entity,
        actor: Entity,
        queue_slot: usize,
        slots: &[WorldObjectInteractionSlotDefinition],
    ) -> bool {
        let Some(queue) = slots.get(queue_slot).filter(|slot| slot.is_queue) else {
            return false;
        };
        self.members_in_slot(target, queue_slot).next() == Some(actor)
            && slots.iter().enumerate().any(|(index, slot)| {
                !slot.is_queue
                    && slot.reservation_tag == queue.reservation_tag
                    && self.occupied_places(target, index) < usize::from(slot.capacity)
            })
    }
}

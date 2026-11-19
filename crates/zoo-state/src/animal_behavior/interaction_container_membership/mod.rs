use bevy::ecs::{
    entity_disabling::Disabled, lifecycle::HookContext, query::Allow,
    relationship::RelationshipTarget, system::SystemParam, world::DeferredWorld,
};
use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectInteractionSlotDefinition, AssetId,
};

pub mod requests;
#[cfg(test)]
mod tests;

#[derive(Component)]
#[relationship(relationship_target = InteractionContainerSlotMembers)]
#[component(on_discard = clear_docking_on_membership_discard)]
pub struct InteractionContainerMember {
    #[relationship]
    slot: Entity,
    admission_order: u64,
}

#[derive(Component)]
#[relationship_target(relationship = InteractionContainerMember)]
pub struct InteractionContainerSlotMembers(Vec<Entity>);

#[derive(Component, Copy, Clone)]
#[relationship(relationship_target = InteractionContainerSlots)]
pub struct InteractionContainerSlotOf {
    #[relationship]
    container: Entity,
    index: usize,
}

#[derive(Component)]
#[relationship_target(relationship = InteractionContainerSlotOf, linked_spawn)]
pub struct InteractionContainerSlots(Vec<Entity>);

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
    all_members: Query<'w, 's, &'static InteractionContainerMember, Allow<Disabled>>,
    containers: Query<'w, 's, &'static InteractionContainerSlots>,
    slots: Query<
        'w,
        's,
        (
            Entity,
            &'static InteractionContainerSlotOf,
            Option<&'static InteractionContainerSlotMembers>,
        ),
    >,
}

impl<'w, 's> InteractionContainerMemberships<'w, 's> {
    #[must_use]
    pub fn container_for_member(&self, member: Entity) -> Option<(Entity, usize)> {
        let (membership, _) = self.members.get(member).ok()?;
        let (_, slot, _) = self.slots.get(membership.slot).ok()?;
        Some((slot.container, slot.index))
    }

    pub fn members_of_container<'a>(
        &'a self,
        container: Entity,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        let mut members = self
            .slots
            .iter_many(
                self.containers
                    .get(container)
                    .into_iter()
                    .flat_map(RelationshipTarget::iter),
            )
            .flatten()
            .flat_map(|(_, _, members)| members.into_iter().flat_map(RelationshipTarget::iter))
            .filter_map(|member| {
                let membership = self.all_members.get(member).ok()?;
                Some((membership.admission_order, member))
            })
            .collect::<Vec<_>>();
        members.sort_unstable_by_key(|&(order, _)| order);
        members.into_iter().map(|(_, member)| member)
    }

    pub fn members_in_slot<'a>(
        &'a self,
        container: Entity,
        slot: usize,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        self.slots
            .iter_many(
                self.containers
                    .get(container)
                    .into_iter()
                    .flat_map(RelationshipTarget::iter),
            )
            .flatten()
            .find_map(|(_, candidate, members)| (candidate.index == slot).then_some(members))
            .flatten()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter(|member| self.members.contains(*member))
    }

    #[must_use]
    pub fn occupied_places(&self, target: Entity, slot: usize) -> usize {
        self.members_in_slot(target, slot).count()
    }

    #[must_use]
    pub fn actor_slot(&self, target: Entity, actor: Entity) -> Option<usize> {
        let (container, slot) = self.container_for_member(actor)?;
        (container == target).then_some(slot)
    }

    pub fn docked_member_entities_in_container<'a>(
        &'a self,
        container: Entity,
    ) -> impl Iterator<Item = Entity> + use<'a, 'w, 's> {
        self.members_of_container(container)
            .filter(move |member| self.members.get(*member).is_ok_and(|(_, docked)| docked))
    }

    #[must_use]
    pub fn is_docked(&self, target: Entity, actor: Entity) -> bool {
        self.members.get(actor).is_ok_and(|(member, docked)| {
            docked
                && self
                    .slots
                    .get(member.slot)
                    .is_ok_and(|(_, slot, _)| slot.container == target)
        })
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

//! Live container membership. Definitions retain capacities and slot policy;
//! this owner retains only actor references and indexes into those definitions.

use bevy::{platform::collections::HashMap, prelude::*};
mod queue_membership;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectInteractionSlotDefinition, AssetId,
};

#[derive(Resource, Default)]
pub(crate) struct InteractionContainerOccupancy {
    members: HashMap<Entity, Vec<(Entity, usize)>>,
    docked: HashMap<Entity, Entity>,
}

impl InteractionContainerOccupancy {
    pub(crate) fn container_for_member(&self, member: Entity) -> Option<(Entity, usize)> {
        self.members.iter().find_map(|(container, members)| {
            members
                .iter()
                .find_map(|(candidate, slot)| (*candidate == member).then_some((*container, *slot)))
        })
    }

    pub(super) fn members_in_slot(
        &self,
        container: Entity,
        slot: usize,
    ) -> impl Iterator<Item = Entity> + '_ {
        self.members
            .get(&container)
            .into_iter()
            .flatten()
            .filter(move |(_, member_slot)| *member_slot == slot)
            .map(|(member, _)| *member)
    }
    /// Reuse membership in the requested container, otherwise choose its nearest
    /// available service socket. Queue slots never admit service users directly.
    pub(super) fn nearest_available_service_slot(
        &self,
        target: Entity,
        actor: Entity,
        slots: &[WorldObjectInteractionSlotDefinition],
        reservation_tag: Option<AssetId>,
        mut ground_offset: impl FnMut(&str) -> Option<Vec2>,
    ) -> Option<usize> {
        let existing = self.actor_slot(target, actor);
        slots
            .iter()
            .enumerate()
            .filter(|(index, slot)| {
                !slot.is_queue
                    && slot.capacity != 0
                    && reservation_tag.is_none_or(|tag| slot.reservation_tag == tag)
                    && (existing == Some(*index)
                        || self.occupied_places(target, *index) < usize::from(slot.capacity))
            })
            .filter_map(|(index, slot)| {
                Some((
                    index,
                    ground_offset(&slot.target_node_name)?.length_squared(),
                ))
            })
            .min_by(|(left, left_distance), (right, right_distance)| {
                (existing != Some(*left))
                    .cmp(&(existing != Some(*right)))
                    .then_with(|| left_distance.total_cmp(right_distance))
            })
            .map(|(index, _)| index)
    }

    pub(super) fn occupied_places(&self, target: Entity, slot: usize) -> usize {
        self.members.get(&target).map_or(0, |members| {
            members
                .iter()
                .filter(|(_, occupied_slot)| *occupied_slot == slot)
                .count()
        })
    }

    pub(super) fn actor_slot(&self, target: Entity, actor: Entity) -> Option<usize> {
        self.members
            .get(&target)?
            .iter()
            .find_map(|(member, slot)| (*member == actor).then_some(*slot))
    }

    /// Admission is synchronous so actors executing in one tick cannot each
    /// reserve the last place before deferred entity commands are applied.
    pub(super) fn admit(
        &mut self,
        target: Entity,
        actor: Entity,
        slot: usize,
        capacity: u8,
    ) -> bool {
        if self.actor_slot(target, actor) == Some(slot) {
            return true;
        }
        if self.occupied_places(target, slot) >= usize::from(capacity) {
            return false;
        }
        self.remove_actor(actor);
        self.members.entry(target).or_default().push((actor, slot));
        true
    }

    pub(super) fn remove_actor(&mut self, actor: Entity) {
        self.docked.remove(&actor);
        self.members.retain(|_, members| {
            // Stable removal preserves queue order.
            members.retain(|(member, _)| *member != actor);
            !members.is_empty()
        });
    }

    /// Returns actors that are actually docked in nonqueue service positions
    /// of this container, in stable occupancy admission order. Queue waiters
    /// are admitted into `members` but are never marked docked.
    pub(crate) fn docked_member_entities_in_container(
        &self,
        container: Entity,
    ) -> impl Iterator<Item = Entity> + '_ {
        self.members
            .get(&container)
            .into_iter()
            .flatten()
            .filter(move |(member, _)| self.docked.get(member) == Some(&container))
            .map(|(member, _)| *member)
    }

    pub(super) fn is_docked(&self, target: Entity, actor: Entity) -> bool {
        self.docked.get(&actor) == Some(&target)
    }

    pub(super) fn mark_docked(&mut self, target: Entity, actor: Entity) {
        if self.actor_slot(target, actor).is_some() {
            self.docked.insert(actor, target);
        }
    }

    pub(super) fn begin_redocking(&mut self, actor: Entity) {
        self.docked.remove(&actor);
    }

    fn remove_destroyed_entities(
        &mut self,
        entities: &Query<()>,
        contained: &Query<(), With<super::behavior_object_attachment_execution::ContainedObject>>,
    ) {
        self.docked
            .retain(|actor, target| entities.contains(*actor) && entities.contains(*target));
        self.members.retain(|target, members| {
            // Carried-object cleanup needs the original slot to apply its
            // authored ownership policy, even while definitions are reloading.
            if !entities.contains(*target)
                && !members
                    .iter()
                    .any(|(member, _)| contained.contains(*member))
            {
                return false;
            }
            members.retain(|(actor, _)| entities.contains(*actor));
            !members.is_empty()
        });
    }
}

pub(super) fn remove_destroyed_container_memberships(
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    entities: Query<()>,
    contained: Query<(), With<super::behavior_object_attachment_execution::ContainedObject>>,
) {
    occupancy.remove_destroyed_entities(&entities, &contained);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_selection_preserves_membership_and_excludes_queue_full_and_other_containers() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let actor = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        let tag = AssetId::from_key("use_stand");
        let mut slots: Vec<_> = (0..4)
            .map(|index| WorldObjectInteractionSlotDefinition {
                reservation_tag: tag,
                is_queue: index == 0,
                capacity: 1,
                exclusive_identifier: AssetId::default(),
                owns_contents: false,
                hides_contents: false,
                entrance_behavior_set: AssetId::default(),
                use_behavior_set: AssetId::default(),
                exit_behavior_set: AssetId::default(),
                target_node_name: index.to_string(),
            })
            .collect();
        slots[3].reservation_tag = AssetId::from_key("other");
        let mut occupancy = InteractionContainerOccupancy::default();
        let position = |name: &str| match name {
            "0" => Some(Vec2::ZERO),
            "1" => Some(Vec2::X),
            "2" => Some(Vec2::X * 2.0),
            "3" => Some(Vec2::X * 3.0),
            _ => None,
        };
        assert_eq!(
            occupancy.nearest_available_service_slot(target, actor, &slots, Some(tag), position),
            Some(1)
        );
        assert!(occupancy.admit(target, other, 1, 1));
        assert_eq!(
            occupancy.nearest_available_service_slot(target, actor, &slots, Some(tag), position),
            Some(2)
        );
        assert!(occupancy.admit(target, actor, 2, 1));
        occupancy.remove_actor(other);
        assert_eq!(
            occupancy.nearest_available_service_slot(target, actor, &slots, Some(tag), position),
            Some(2)
        );
        assert_eq!(
            occupancy.nearest_available_service_slot(target, other, &slots, Some(tag), |_| None),
            None
        );
    }
}

//! Queue order is the stable order of the container's existing memberships.

use super::InteractionContainerOccupancy;
use bevy::prelude::Entity;
use openzt2_game_data::world_definitions::world_objects::WorldObjectInteractionSlotDefinition;

impl InteractionContainerOccupancy {
    /// Only the front member may leave the queue for an available service slot
    /// in the same authored container. Admission itself remains synchronous.
    pub(in crate::plugins::animal_behavior) fn queue_front_can_enter_service(
        &self,
        target: Entity,
        actor: Entity,
        queue_slot: usize,
        slots: &[WorldObjectInteractionSlotDefinition],
    ) -> bool {
        let Some(queue) = slots.get(queue_slot).filter(|slot| slot.is_queue) else {
            return false;
        };
        let front = self
            .members
            .get(&target)
            .and_then(|members| members.iter().find(|(_, slot)| *slot == queue_slot));
        front.is_some_and(|(member, _)| *member == actor)
            && slots.iter().enumerate().any(|(index, slot)| {
                !slot.is_queue
                    && slot.reservation_tag == queue.reservation_tag
                    && self.occupied_places(target, index) < usize::from(slot.capacity)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::World;
    use openzt2_game_data::AssetId;

    #[test]
    fn only_queue_front_can_use_free_capacity_in_its_own_container() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let serving = world.spawn_empty().id();
        let tag = AssetId::from_key("food");
        let mut slots: Vec<_> = (0..3)
            .map(|index| WorldObjectInteractionSlotDefinition {
                reservation_tag: tag,
                is_queue: index == 0,
                capacity: if index == 0 { 5 } else { 1 },
                exclusive_identifier: AssetId::default(),
                owns_contents: false,
                hides_contents: false,
                entrance_behavior_set: AssetId::default(),
                use_behavior_set: AssetId::default(),
                exit_behavior_set: AssetId::default(),
                target_node_name: String::new(),
            })
            .collect();
        slots[2].reservation_tag = AssetId::from_key("drinks");
        let mut occupancy = InteractionContainerOccupancy::default();
        assert!(occupancy.admit(target, first, 0, 5));
        assert!(occupancy.admit(target, second, 0, 5));
        assert!(occupancy.queue_front_can_enter_service(target, first, 0, &slots));
        assert!(!occupancy.queue_front_can_enter_service(target, second, 0, &slots));
        assert!(occupancy.admit(target, serving, 1, 1));
        assert!(!occupancy.queue_front_can_enter_service(target, first, 0, &slots));
        occupancy.remove_actor(serving);
        assert!(occupancy.admit(target, first, 1, 1));
        assert!(!occupancy.queue_front_can_enter_service(target, second, 0, &slots));
        occupancy.remove_actor(first);
        assert!(occupancy.queue_front_can_enter_service(target, second, 0, &slots));
    }
}

use bevy::{
    ecs::{relationship::RelationshipTarget, system::Command},
    prelude::*,
};
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};

use super::{
    InteractionContainerMember, InteractionContainerSlotMembers, InteractionContainerSlotOf,
    InteractionContainerSlots,
};

pub struct ReleaseInteractionContainerMember {
    pub member: Entity,
    pub despawn: bool,
}

pub enum InteractionContainerAdmissionRequest {
    Admit {
        container: Entity,
        member: Entity,
        slots: Vec<(usize, u8)>,
        despawn_on_refusal: bool,
        queue_service_slots: Vec<(usize, u8)>,
    },
    AdmitNew {
        container: Entity,
        slot: usize,
        capacity: u8,
        world_root: Entity,
    },
}

#[derive(Clone, Copy)]
pub enum InteractionContainerAdmissionOutcome {
    Granted {
        member: Entity,
        slot: usize,
        persistent_id: Option<PersistentId>,
        queue_service_available: bool,
    },
    Refused,
    IdentityUnavailable,
}

impl Command for ReleaseInteractionContainerMember {
    type Out = ();

    fn apply(self, world: &mut World) {
        release_container_member(world, self.member, self.despawn);
    }
}

fn release_container_member(world: &mut World, member: Entity, despawn: bool) {
    if let Ok(mut member) = world.get_entity_mut(member) {
        if despawn {
            member.despawn();
        } else {
            member.remove::<InteractionContainerMember>();
        }
    }
}

fn find_container_slot(world: &World, container: Entity, index: usize) -> Option<Entity> {
    world
        .get::<InteractionContainerSlots>(container)?
        .iter()
        .find(|slot| {
            world
                .get::<InteractionContainerSlotOf>(*slot)
                .is_some_and(|slot| slot.index == index)
        })
}

fn slot_members(world: &World, slot: Option<Entity>) -> Option<&InteractionContainerSlotMembers> {
    world.get::<InteractionContainerSlotMembers>(slot?)
}

#[derive(Component, Default)]
struct InteractionContainerAdmissionSequence(u64);

fn admit_to_container_slot(
    world: &mut World,
    container: Entity,
    index: usize,
    slot: Option<Entity>,
    member: Option<Entity>,
) -> Entity {
    let slot = slot.unwrap_or_else(|| {
        world
            .spawn(InteractionContainerSlotOf { container, index })
            .id()
    });
    let admission_order = if let Some(mut sequence) =
        world.get_mut::<InteractionContainerAdmissionSequence>(container)
    {
        sequence.0 = sequence
            .0
            .checked_add(1)
            .expect("interaction admission sequence exhausted");
        sequence.0
    } else {
        world
            .entity_mut(container)
            .insert(InteractionContainerAdmissionSequence(1));
        1
    };
    let membership = InteractionContainerMember {
        slot,
        admission_order,
    };
    let member = if let Some(member) = member {
        world.entity_mut(member).insert(membership);
        member
    } else {
        world.spawn(membership).id()
    };
    world.flush();
    member
}

impl InteractionContainerAdmissionRequest {
    /// Applies this structural request against committed container membership.
    pub fn commit(self, world: &mut World) -> InteractionContainerAdmissionOutcome {
        use InteractionContainerAdmissionOutcome::{Granted, IdentityUnavailable, Refused};

        match self {
            Self::Admit {
                container,
                member,
                slots,
                despawn_on_refusal,
                queue_service_slots,
            } => {
                if world.entities().contains_spawned(container)
                    && world.entities().contains_spawned(member)
                {
                    let existing = world
                        .get::<InteractionContainerMember>(member)
                        .map(|membership| membership.slot);
                    for (slot, capacity) in slots {
                        let slot_entity = find_container_slot(world, container, slot);
                        if existing.is_none() || existing != slot_entity {
                            if slot_members(world, slot_entity).map_or(0, RelationshipTarget::len)
                                >= usize::from(capacity)
                            {
                                continue;
                            }
                            admit_to_container_slot(
                                world,
                                container,
                                slot,
                                slot_entity,
                                Some(member),
                            );
                        }
                        let queue_service_available = !queue_service_slots.is_empty()
                            && slot_members(world, find_container_slot(world, container, slot))
                                .and_then(|members| members.iter().next())
                                == Some(member)
                            && queue_service_slots.iter().any(|&(slot, capacity)| {
                                slot_members(world, find_container_slot(world, container, slot))
                                    .map_or(0, RelationshipTarget::len)
                                    < usize::from(capacity)
                            });
                        return Granted {
                            member,
                            slot,
                            persistent_id: None,
                            queue_service_available,
                        };
                    }
                }
                if despawn_on_refusal {
                    release_container_member(world, member, true);
                }
                Refused
            }
            Self::AdmitNew {
                container,
                slot,
                capacity,
                world_root,
            } => {
                let slot_entity = find_container_slot(world, container, slot);
                if !world.entities().contains_spawned(container)
                    || slot_members(world, slot_entity).map_or(0, RelationshipTarget::len)
                        >= usize::from(capacity)
                {
                    return Refused;
                }
                let Some(persistent_id) = world
                    .get_mut::<PersistentIdAllocator>(world_root)
                    .and_then(|mut ids| ids.allocate().ok())
                else {
                    return IdentityUnavailable;
                };
                let member = admit_to_container_slot(world, container, slot, slot_entity, None);
                Granted {
                    member,
                    slot,
                    persistent_id: Some(persistent_id),
                    queue_service_available: false,
                }
            }
        }
    }
}

use bevy::{
    ecs::{
        entity::{Entities, EntityHashMap, EntityHashSet},
        entity_disabling::Disabled,
        query::Allow,
        relationship::RelationshipTarget,
        system::SystemParam,
    },
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};

use super::{InteractionContainerMember, InteractionContainerMembers};

#[derive(Message)]
pub enum InteractionContainerRequest {
    Admit {
        admission: InteractionContainerAdmissionRequest,
        reply_to: InteractionContainerAdmissionRecipient,
    },
    Release {
        member: Entity,
        despawn: bool,
    },
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
pub enum InteractionContainerAdmissionRecipient {
    Docking(Entity),
    Entry(Entity),
    QueueWait(Entity),
    Attachment(Entity),
}

#[derive(Message)]
pub struct InteractionContainerAdmission {
    pub reply_to: InteractionContainerAdmissionRecipient,
    pub outcome: InteractionContainerAdmissionOutcome,
}

#[derive(Message)]
pub struct InteractionContainerAttachmentAdmission {
    pub request_entity: Entity,
    pub outcome: InteractionContainerAdmissionOutcome,
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

pub fn register_interaction_container_requests(app: &mut App) {
    app.add_message::<InteractionContainerRequest>()
        .add_message::<InteractionContainerAdmission>()
        .add_message::<InteractionContainerAttachmentAdmission>();
}

pub fn interaction_container_requests_pending(
    requests: Res<Messages<InteractionContainerRequest>>,
) -> bool {
    !requests.is_empty()
}

pub fn resolve_interaction_container_requests(
    mut requests: ResMut<Messages<InteractionContainerRequest>>,
    mut admissions: MessageWriter<InteractionContainerAdmission>,
    mut attachments: MessageWriter<InteractionContainerAttachmentAdmission>,
    mut resolver: InteractionContainerRequestResolver,
    mut commands: Commands,
) {
    resolver.begin_batch();
    // Several fixed-tick boundaries drain the same queue.
    for request in requests.drain() {
        match request {
            InteractionContainerRequest::Admit {
                admission,
                reply_to,
            } => {
                let outcome = resolver.admit(&admission, &mut commands);
                if let InteractionContainerAdmissionRecipient::Attachment(request_entity) = reply_to
                {
                    attachments.write(InteractionContainerAttachmentAdmission {
                        request_entity,
                        outcome,
                    });
                } else {
                    admissions.write(InteractionContainerAdmission { reply_to, outcome });
                }
            }
            InteractionContainerRequest::Release { member, despawn } => {
                resolver.release(member, despawn, &mut commands);
            }
        }
    }
}

// The relationship commands apply after the batch. Project only the members
// and slot counts touched by this batch so each request sees earlier changes.
#[derive(Default)]
struct PendingMembershipChanges {
    members: EntityHashMap<Option<(InteractionContainerMember, usize)>>,
    places: HashMap<(Entity, usize), PendingSlotOccupancy>,
    despawned: EntityHashSet,
    next_admission: usize,
}

struct PendingSlotOccupancy {
    count: usize,
    first_admission: Option<(Entity, usize)>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct LiveMemberships<'w, 's> {
    members: Query<'w, 's, &'static InteractionContainerMember, Allow<Disabled>>,
    containers: Query<'w, 's, &'static InteractionContainerMembers, Allow<Disabled>>,
    entities: &'w Entities,
}

impl PendingMembershipChanges {
    fn slot_occupancy(
        &mut self,
        live: &LiveMemberships,
        membership: InteractionContainerMember,
    ) -> &mut PendingSlotOccupancy {
        self.places
            .entry((membership.container, membership.slot))
            .or_insert_with(|| PendingSlotOccupancy {
                count: live
                    .containers
                    .get(membership.container)
                    .into_iter()
                    .flat_map(RelationshipTarget::iter)
                    .filter(|member| {
                        live.members
                            .get(*member)
                            .is_ok_and(|entry| *entry == membership)
                    })
                    .count(),
                first_admission: None,
            })
    }

    fn membership_of(
        &self,
        live: &LiveMemberships,
        member: Entity,
    ) -> Option<InteractionContainerMember> {
        self.members
            .get(&member)
            .map_or_else(
                || live.members.get(member).ok().copied(),
                |entry| entry.map(|(membership, _)| membership),
            )
            .filter(|membership| !self.despawned.contains(&membership.container))
    }

    fn set_membership(
        &mut self,
        live: &LiveMemberships,
        member: Entity,
        next: Option<InteractionContainerMember>,
    ) {
        let previous = self.membership_of(live, member);
        if previous == next {
            return;
        }
        if let Some(previous) = previous {
            self.slot_occupancy(live, previous).count -= 1;
        }
        let next = next.map(|next| {
            let admission = self.next_admission;
            self.next_admission += 1;
            let occupancy = self.slot_occupancy(live, next);
            occupancy.count += 1;
            occupancy.first_admission.get_or_insert((member, admission));
            (next, admission)
        });
        self.members.insert(member, next);
    }

    // A waiter sees the queue and service capacity at its own admission.
    fn queue_front_can_enter_service(
        &mut self,
        live: &LiveMemberships,
        member: Entity,
        queue: InteractionContainerMember,
        service: &[(usize, u8)],
    ) -> bool {
        if service.is_empty() {
            return false;
        }
        let live_front = live
            .containers
            .get(queue.container)
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .find(|entity| {
                !self.members.contains_key(entity)
                    && live.members.get(*entity).is_ok_and(|entry| *entry == queue)
            });
        let front = live_front.or_else(|| self.first_projected_admission(queue));
        front == Some(member)
            && service.iter().any(|&(slot, capacity)| {
                self.slot_occupancy(live, InteractionContainerMember { slot, ..queue })
                    .count
                    < usize::from(capacity)
            })
    }

    fn first_projected_admission(&mut self, slot: InteractionContainerMember) -> Option<Entity> {
        let occupancy = self.places.get_mut(&(slot.container, slot.slot))?;
        let (entity, order) = occupancy.first_admission?;
        if self.members.get(&entity) == Some(&Some((slot, order))) {
            return Some(entity);
        }
        occupancy.first_admission = self
            .members
            .iter()
            .filter_map(|(&entity, &entry)| {
                let (membership, order) = entry?;
                (membership == slot).then_some((entity, order))
            })
            .min_by_key(|&(_, order)| order);
        occupancy.first_admission.map(|(entity, _)| entity)
    }

    fn release(
        &mut self,
        live: &LiveMemberships,
        member: Entity,
        despawn: bool,
        commands: &mut Commands,
    ) {
        if !live.entities.contains_spawned(member) || self.despawned.contains(&member) {
            return;
        }
        self.set_membership(live, member, None);
        if despawn {
            self.despawned.insert(member);
            commands.entity(member).try_despawn();
        } else {
            commands
                .entity(member)
                .try_remove::<InteractionContainerMember>();
        }
    }
}

#[derive(SystemParam)]
pub struct InteractionContainerRequestResolver<'w, 's> {
    live: LiveMemberships<'w, 's>,
    changes: Local<'s, PendingMembershipChanges>,
    allocator: Option<ResMut<'w, PersistentIdAllocator>>,
}

impl InteractionContainerRequestResolver<'_, '_> {
    pub fn begin_batch(&mut self) {
        self.changes.members.clear();
        self.changes.places.clear();
        self.changes.despawned.clear();
        self.changes.next_admission = 0;
    }

    pub fn release(&mut self, member: Entity, despawn: bool, commands: &mut Commands) {
        self.changes.release(&self.live, member, despawn, commands);
    }

    pub fn admit(
        &mut self,
        request: &InteractionContainerAdmissionRequest,
        commands: &mut Commands,
    ) -> InteractionContainerAdmissionOutcome {
        use InteractionContainerAdmissionOutcome::{Granted, IdentityUnavailable, Refused};

        let live = &self.live;
        let changes = &mut *self.changes;
        match *request {
            InteractionContainerAdmissionRequest::Admit {
                container,
                member,
                ref slots,
                despawn_on_refusal,
                ref queue_service_slots,
            } => {
                let selected = if live.entities.contains_spawned(container)
                    && live.entities.contains_spawned(member)
                    && !changes.despawned.contains(&container)
                    && !changes.despawned.contains(&member)
                {
                    let previous = changes.membership_of(live, member);
                    slots.iter().find_map(|&(slot, capacity)| {
                        let membership = InteractionContainerMember { container, slot };
                        if previous != Some(membership) {
                            if changes.slot_occupancy(live, membership).count
                                >= usize::from(capacity)
                            {
                                return None;
                            }
                            changes.set_membership(live, member, Some(membership));
                            commands.entity(member).try_insert(membership);
                        }
                        Some(slot)
                    })
                } else {
                    None
                };
                if let Some(slot) = selected {
                    Granted {
                        member,
                        slot,
                        persistent_id: None,
                        queue_service_available: changes.queue_front_can_enter_service(
                            live,
                            member,
                            InteractionContainerMember { container, slot },
                            queue_service_slots,
                        ),
                    }
                } else {
                    if despawn_on_refusal {
                        changes.release(live, member, true, commands);
                    }
                    Refused
                }
            }
            InteractionContainerAdmissionRequest::AdmitNew {
                container,
                slot,
                capacity,
                world_root,
            } => {
                let membership = InteractionContainerMember { container, slot };
                if !live.entities.contains_spawned(container)
                    || changes.despawned.contains(&container)
                    || changes.slot_occupancy(live, membership).count >= usize::from(capacity)
                {
                    Refused
                } else if let Some(persistent_id) = self
                    .allocator
                    .as_deref_mut()
                    .and_then(|ids| ids.allocate(world_root).ok())
                {
                    let member = commands.spawn(membership).id();
                    changes.set_membership(live, member, Some(membership));
                    Granted {
                        member,
                        slot,
                        persistent_id: Some(persistent_id),
                        queue_service_available: false,
                    }
                } else {
                    IdentityUnavailable
                }
            }
        }
    }
}

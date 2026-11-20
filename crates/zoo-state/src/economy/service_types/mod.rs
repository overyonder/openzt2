use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceOperation {
    pub service: AssetId,
    pub inventory_units: u16,
    pub phase: ServicePhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServicePhase {
    Timing(u32),
    AwaitingPayment,
    Completed,
}

#[derive(Component)]
#[relationship(relationship_target = ServiceReservation)]
pub struct ServiceCustomer(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ServiceCustomer, linked_spawn)]
pub struct ServiceReservation(Entity);

#[derive(Component)]
#[relationship(relationship_target = FacilityReservations)]
pub struct ReservationFacility(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ReservationFacility, linked_spawn)]
pub struct FacilityReservations(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingServiceRequest {
    pub sequence: u64,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceCompleted {
    pub customer: Entity,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Resource, Debug, Default)]
pub struct ServiceRequestSequence {
    next: u64,
}

impl ServiceRequestSequence {
    pub fn allocate(&mut self) -> Option<u64> {
        let sequence = self.next;
        self.next = self.next.checked_add(1)?;
        Some(sequence)
    }

    pub fn from_snapshot(next: u64) -> Self {
        Self { next }
    }

    pub fn next_sequence(&self) -> u64 {
        self.next
    }
}

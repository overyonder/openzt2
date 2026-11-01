use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceReservation {
    pub facility: Entity,
    pub service: AssetId,
    pub inventory_units: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceProgress {
    pub facility: Entity,
    pub remaining_ticks: u32,
}

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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePaymentPending {
    pub customer: Entity,
    pub facility: Entity,
    pub service: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePaymentInFlight(pub Entity);

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

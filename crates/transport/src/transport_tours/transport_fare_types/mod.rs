use bevy::prelude::*;

use openzt2_zoo_state::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[relationship(relationship_target = FareOperations)]
pub(super) struct FareGuest(pub(super) Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingTransportFare {
    pub(super) circuit: Entity,
    pub(super) score: f32,
    pub(super) rating: f32,
}

#[derive(Component)]
#[relationship_target(relationship = FareGuest, linked_spawn)]
pub(super) struct FareOperations(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct QueuedFare {
    pub(super) amount: Money,
    pub(super) settled: bool,
}

impl QueuedFare {
    pub(super) const fn pending(amount: Money) -> Self {
        Self {
            amount,
            settled: false,
        }
    }
}

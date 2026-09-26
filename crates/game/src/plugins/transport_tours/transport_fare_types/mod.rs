use bevy::prelude::*;

use crate::plugins::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingTransportFare {
    pub(super) guest: Entity,
    pub(super) circuit: Entity,
    pub(super) score: f32,
    pub(super) rating: f32,
}

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

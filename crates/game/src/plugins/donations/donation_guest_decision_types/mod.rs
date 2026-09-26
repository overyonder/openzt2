use bevy::prelude::*;

use crate::plugins::{
    economy::money_types::Money, simulation_time::deterministic_random_stream::DeterministicRng,
};

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct DonationRng(pub(super) DeterministicRng);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationCandidate {
    pub(super) navigation_request_id: u64,
    pub(super) acceptor: Entity,
    pub(super) subject: Entity,
    pub(super) amount: Money,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationProgress {
    pub(super) acceptor: Entity,
    pub(super) subject: Entity,
    pub(super) amount: Money,
    pub(super) remaining_ticks: u32,
}

/// One guest's latest donation observation from viewing a subject.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationObservation {
    pub(super) subject: Entity,
    pub(super) amount: Money,
    pub(super) minimum_satisfaction_permille: u16,
    pub(super) chance_permyriad: u16,
    pub(super) acceptor_radius_cells: u8,
}

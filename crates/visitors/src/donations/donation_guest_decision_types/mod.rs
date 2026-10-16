use bevy::prelude::*;

use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::guests::guest_simulation_types::Viewing;

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
    pub(super) viewing: Viewing,
    pub(super) amount: Money,
    pub(super) acceptor_radius_cm: u32,
}

use bevy::prelude::*;

use crate::economy::money_types::Money;
use crate::guests::guest_simulation_types::Viewing;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationCandidate {
    pub navigation_request_id: u64,
    pub acceptor: Entity,
    pub subject: Entity,
    pub amount: Money,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationProgress {
    pub acceptor: Entity,
    pub subject: Entity,
    pub amount: Money,
    pub remaining_ticks: u32,
}

/// One guest's latest donation observation from viewing a subject.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationObservation {
    pub viewing: Viewing,
    pub amount: Money,
    pub acceptor_radius_cm: u32,
}

#[derive(Component, Clone, Default)]
pub struct GuestDonationMeters(pub std::collections::HashMap<openzt2_game_data::AssetId, f32>);

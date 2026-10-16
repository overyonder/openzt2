use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationAcceptor {
    pub beneficiary: Option<Entity>,
}

/// Records that one spawned object has been classified against its authored
/// donation-acceptor flag. Objects with the flag wait for their prefab's
/// authored docking attachment before this marker is inserted.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DonationAcceptorFactsResolved;

/// Allows an experience to settle donations without docking at a box.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirectDonationSettlement;

/// Authored docking geometry for a donation acceptor.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct DonationUsePoint {
    pub local_point: Vec3,
    pub local_forward: Vec3,
}

/// Donation policy for a viewable subject, show, or tour.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationOpportunity {
    pub category: AssetId,
    pub amount: Money,
    pub interaction_ticks: u32,
}

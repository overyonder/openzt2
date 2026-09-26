use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DonationAcceptor {
    pub(crate) beneficiary: Option<Entity>,
}

/// Records that one spawned object has been classified against its authored
/// donation-acceptor flag. Objects with the flag wait for their prefab's
/// authored docking attachment before this marker is inserted.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct DonationAcceptorFactsResolved;

/// Allows an experience to settle donations without docking at a box.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DirectDonationSettlement;

/// Authored docking geometry for a donation acceptor.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct DonationUsePoint {
    pub(super) local_point: Vec3,
    pub(super) local_forward: Vec3,
}

/// Donation policy for a viewable subject, show, or tour.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DonationOpportunity {
    pub(crate) category: AssetId,
    pub(crate) amount: Money,
    pub(crate) minimum_satisfaction_permille: u16,
    pub(crate) chance_permyriad: u16,
    pub(crate) interaction_ticks: u32,
    pub(crate) acceptor_radius_cells: u8,
}

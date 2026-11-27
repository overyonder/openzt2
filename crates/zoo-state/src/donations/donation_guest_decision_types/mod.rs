use bevy::prelude::*;

use crate::economy::money_types::Money;

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

#[derive(Component, Clone, Default)]
pub struct GuestDonationMeters(pub std::collections::HashMap<openzt2_game_data::AssetId, f32>);

#[derive(Component, Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct SpeciesDonationCandidateLifetimes(
    pub std::collections::BTreeMap<openzt2_game_data::AssetId, SpeciesDonationCandidate>,
);

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct SpeciesDonationCandidate {
    pub binder: openzt2_game_data::AssetId,
    pub remaining: f32,
}

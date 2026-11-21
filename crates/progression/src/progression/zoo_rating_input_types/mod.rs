use bevy::prelude::*;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct AnimalWelfareRating(pub(super) u16);

#[derive(Resource, Default, PartialEq)]
pub(super) struct LivingAnimalPopulation {
    pub(super) animals: u32,
    pub(super) species: u32,
    pub(super) observations: Vec<AnimalObservation>,
}

#[derive(Resource, Default, PartialEq)]
pub(super) struct GuestRatingFacts {
    pub(super) count: u32,
    pub(super) satisfaction: u16,
    pub(super) education: u16,
    pub(super) observations: Vec<GuestObservation>,
    pub(super) species_seen: BTreeMap<AssetId, SeenSpecies>,
}

#[derive(PartialEq)]
pub(super) struct AnimalObservation {
    pub(super) species: AssetId,
    pub(super) happiness: f32,
}

#[derive(PartialEq)]
pub(super) struct GuestObservation {
    pub(super) happiness: f32,
    pub(super) education: f32,
}

#[derive(Default, PartialEq)]
pub(super) struct SeenSpecies {
    pub(super) guests: u32,
    pub(super) events: u32,
    pub(super) entertainment: f32,
    pub(super) donation: f32,
}

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct EarnedAwardFamePoints(pub(super) u32);

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct ZooSceneryRating(pub(super) u16);

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct ZooFinanceRating(pub(super) u16);

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct ZooOverallRatingTarget(pub(super) Option<(u16, u32)>);

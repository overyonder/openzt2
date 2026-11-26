use bevy::prelude::*;

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct AnimalWelfareRating(pub(super) u16);

#[derive(Component, Default, PartialEq)]
pub(super) struct LivingAnimalPopulation {
    pub(super) animals: u32,
    pub(super) species: u32,
}

#[derive(Component, Default, PartialEq)]
pub(super) struct GuestRatingFacts {
    pub(super) count: u32,
    pub(super) satisfaction: u16,
    pub(super) education: u16,
}

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct ZooSceneryRating(pub(super) u16);

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct ZooFinanceRating(pub(super) Option<u16>);

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct ZooOverallRatingTarget(pub(super) Option<(u16, u32)>);

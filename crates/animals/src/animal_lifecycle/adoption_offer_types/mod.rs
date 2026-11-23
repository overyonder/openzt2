use std::time::Duration;

use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component)]
pub struct AdoptionOfferRandom(pub DeterministicRng);

#[derive(Component)]
#[relationship(relationship_target = AdoptionOfferOptions)]
pub struct AdoptionOfferOption {
    #[relationship]
    pub world: Entity,
    pub slot: u16,
    pub option: u8,
}

#[derive(Component)]
#[relationship_target(relationship = AdoptionOfferOption, linked_spawn)]
pub struct AdoptionOfferOptions(Vec<Entity>);

#[derive(Component)]
pub struct AdoptionOfferUnlocked;

#[derive(Component)]
pub struct AdoptionOfferCooldown(pub(crate) Timer);

impl AdoptionOfferCooldown {
    #[must_use]
    pub fn from_remaining_seconds(seconds: f64) -> Self {
        Self(offer_timer(seconds))
    }

    #[must_use]
    pub fn remaining_seconds(&self) -> f32 {
        self.0.remaining_secs()
    }
}

#[derive(Component)]
pub struct AdoptionOfferTiming {
    pub(crate) unlock: Option<Timer>,
    pub(crate) retirement: Option<Timer>,
}

impl AdoptionOfferTiming {
    #[must_use]
    pub fn from_remaining_seconds(unlock: f64, retirement: Option<f64>) -> Self {
        Self {
            unlock: (unlock > 0.0).then(|| offer_timer(unlock)),
            retirement: retirement.map(offer_timer),
        }
    }

    #[must_use]
    pub fn remaining_seconds(&self) -> (f32, Option<f32>) {
        (
            self.unlock.as_ref().map_or(0.0, Timer::remaining_secs),
            self.retirement.as_ref().map(Timer::remaining_secs),
        )
    }
}

fn offer_timer(seconds: f64) -> Timer {
    Timer::new(
        Duration::try_from_secs_f64(seconds).unwrap_or_default(),
        TimerMode::Once,
    )
}

#[derive(Component)]
#[relationship(relationship_target = CurrentAdoptionOffer)]
pub struct AdoptionOfferForOption(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AdoptionOfferForOption)]
pub struct CurrentAdoptionOffer(Entity);

#[derive(Component)]
pub struct AdoptionOfferSupply {
    pub species: AssetId,
    pub available_by_sex: [u16; 2],
    pub dismiss_cooldown: f32,
    pending_by_sex: [u16; 2],
}

impl AdoptionOfferSupply {
    pub fn new(species: AssetId, available_by_sex: [u16; 2], dismiss_cooldown: f32) -> Self {
        Self {
            species,
            available_by_sex,
            dismiss_cooldown,
            pending_by_sex: [0; 2],
        }
    }

    pub(crate) fn reserve(&mut self, sex_index: usize) {
        self.available_by_sex[sex_index] -= 1;
        self.pending_by_sex[sex_index] += 1;
    }

    pub(crate) fn settle(&mut self, sex_index: usize, purchased: bool) {
        if self.pending_by_sex[sex_index] != 0 {
            self.pending_by_sex[sex_index] -= 1;
            if !purchased {
                self.available_by_sex[sex_index] += 1;
            }
        }
    }

    pub(crate) fn has_pending_claims(&self) -> bool {
        self.pending_by_sex != [0; 2]
    }
}

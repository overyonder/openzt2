use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component)]
pub struct AdoptionOfferClock(pub f64);

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
pub struct AdoptionOfferCooldown(pub f64);

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
    pub unlock_at: f64,
    pub remove_at: Option<f64>,
    pub dismiss_cooldown: f32,
    pending_by_sex: [u16; 2],
}

impl AdoptionOfferSupply {
    pub fn new(
        species: AssetId,
        available_by_sex: [u16; 2],
        unlock_at: f64,
        remove_at: Option<f64>,
        dismiss_cooldown: f32,
    ) -> Self {
        Self {
            species,
            available_by_sex,
            unlock_at,
            remove_at,
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

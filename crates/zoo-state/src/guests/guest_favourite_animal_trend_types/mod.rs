use bevy::prelude::Component;
use openzt2_game_data::{
    AssetId,
    world_definitions::guest_favourite_animal_trend_policy::GuestFavouriteAnimalTrendPolicy,
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuestFavouriteAnimalTrend {
    species: [Option<AssetId>; 2],
    spawn_count: u32,
    spawn_threshold_index: u8,
}

impl GuestFavouriteAnimalTrend {
    pub const fn new(first_species: Option<AssetId>) -> Self {
        Self {
            species: [first_species, None],
            spawn_count: 0,
            spawn_threshold_index: 0,
        }
    }

    pub const fn from_saved_state(
        species: [Option<AssetId>; 2],
        spawn_count: u32,
        spawn_threshold_index: u8,
    ) -> Option<Self> {
        if spawn_threshold_index > 1 {
            return None;
        }
        Some(Self {
            species,
            spawn_count,
            spawn_threshold_index,
        })
    }

    pub const fn species(self) -> [Option<AssetId>; 2] {
        self.species
    }
    pub const fn spawn_count(self) -> u32 {
        self.spawn_count
    }
    pub const fn spawn_threshold_index(self) -> u8 {
        self.spawn_threshold_index
    }

    pub fn advance_after_guest_spawn(
        &mut self,
        policy: &GuestFavouriteAnimalTrendPolicy,
        choose_replacement: impl FnOnce(Option<AssetId>) -> Option<AssetId>,
    ) {
        self.spawn_count = self.spawn_count.wrapping_add(1);
        if self.spawn_count != policy.spawn_thresholds[usize::from(self.spawn_threshold_index)] {
            return;
        }
        if let Some(second) = self.species[1] {
            self.species = [Some(second), None];
        } else {
            self.species[1] = choose_replacement(self.species[0]);
        }
        self.spawn_count = 0;
        self.spawn_threshold_index = u8::from(self.spawn_threshold_index == 0);
    }
}

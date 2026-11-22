use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub struct DiseaseResearchFunding(pub f32);

impl Default for DiseaseResearchFunding {
    fn default() -> Self {
        Self(0.5)
    }
}

#[derive(Component)]
pub struct DiseaseExposureDeadline(pub u64);

#[derive(Component)]
pub struct ActiveZooDisease {
    pub definition: AssetId,
    pub cure_due_tick: u64,
}

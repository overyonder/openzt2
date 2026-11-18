use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component, Clone, Copy)]
pub(super) struct AmbientAnimalSpawner {
    pub(super) next_tick: u64,
    pub(super) random: DeterministicRng,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AmbientAnimal {
    pub(super) despawn_tick: u64,
}

#[derive(Component)]
pub(super) struct AmbientAnimalPopulation(pub(super) AssetId);

#[derive(Component)]
#[relationship(relationship_target = AmbientAnimalPopulationMembers)]
pub(super) struct AmbientAnimalPopulationMember(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = AmbientAnimalPopulationMember, linked_spawn)]
pub(super) struct AmbientAnimalPopulationMembers(Vec<Entity>);

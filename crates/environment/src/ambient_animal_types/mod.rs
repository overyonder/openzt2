use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;

#[derive(Component)]
pub(super) struct AmbientSpawnCadence {
    pub(super) timer: Timer,
    pub(super) random: DeterministicRng,
}

#[derive(Component)]
pub(super) struct AmbientAnimal {
    pub(super) definition: AssetId,
    pub(super) despawn_tick: Option<u64>,
}

#[derive(Component)]
#[relationship(relationship_target = WorldAmbientAnimals)]
pub(super) struct AmbientAnimalOfWorld(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = AmbientAnimalOfWorld, linked_spawn)]
pub(super) struct WorldAmbientAnimals(Vec<Entity>);

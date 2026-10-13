use bevy::prelude::*;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_employment_types::Staff;

use super::behavior_random_stream_state::BehaviorRandomStream;

pub(super) fn attach_behavior_random_stream_to_animals_and_staff(
    zoo_seed: Res<ZooSeed>,
    actors_without_behavior_random_stream: Query<
        (Entity, &PersistentId),
        (
            Or<(With<Animal>, With<Staff>)>,
            Without<BehaviorRandomStream>,
        ),
    >,
    mut commands: Commands,
) {
    for (actor_entity, persistent_identifier) in &actors_without_behavior_random_stream {
        commands
            .entity(actor_entity)
            .insert(BehaviorRandomStream(DeterministicRng::from_entity(
                *zoo_seed,
                *persistent_identifier,
                RngDomain::Behavior,
            )));
    }
}

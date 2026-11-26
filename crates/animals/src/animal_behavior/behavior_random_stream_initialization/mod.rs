use bevy::prelude::*;

use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_employment_types::Staff;

use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_set_start_request_types::AmbientBehaviorSubject;

pub(super) fn attach_behavior_random_stream_to_animals_and_staff(
    seeds: Query<&ZooSeed>,
    actors_without_behavior_random_stream: Query<
        (
            Entity,
            Option<&PersistentId>,
            &WorldMember,
            Has<AmbientBehaviorSubject>,
        ),
        (
            Or<(With<Animal>, With<Staff>, With<AmbientBehaviorSubject>)>,
            Without<BehaviorRandomStream>,
        ),
    >,
    mut commands: Commands,
) {
    for (actor_entity, persistent_identifier, member, ambient) in
        &actors_without_behavior_random_stream
    {
        let Some(identity) = persistent_identifier
            .copied()
            .or_else(|| ambient.then_some(PersistentId(actor_entity.to_bits())))
        else {
            continue;
        };
        let Ok(seed) = seeds.get(member.root) else {
            continue;
        };
        commands
            .entity(actor_entity)
            .insert(BehaviorRandomStream(DeterministicRng::from_entity(
                *seed,
                identity,
                RngDomain::Behavior,
            )));
    }
}

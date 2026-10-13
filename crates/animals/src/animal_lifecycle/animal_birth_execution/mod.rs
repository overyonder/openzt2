use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;

use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;

use super::{
    animal_birth_and_release_contracts::AnimalBorn,
    animal_entity_spawning::spawn_animal_entity_from_resolved_components,
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
    types::{Animal, Parents, Pregnancy, ReproductionTraits, SpeciesHandle},
};

pub(super) fn complete_due_animal_pregnancies(
    mut commands: Commands,
    clock: Res<ZooClock>,
    seed: Res<ZooSeed>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut identifiers: ResMut<PersistentIdAllocator>,
    mothers: Query<
        (
            Entity,
            &PersistentId,
            &Pregnancy,
            &ReproductionTraits,
            &SpeciesHandle,
            &WorldMember,
            &Transform,
            Option<&HabitatMember>,
            Has<Dead>,
        ),
        With<Animal>,
    >,
    animals: Query<&WorldMember, With<Animal>>,
    mut births: MessageWriter<AnimalBorn>,
) {
    // Persistent ordering keeps child identifiers independent of ECS insertion
    // order, including after restoration into a different world.
    let mut mothers = mothers.iter().collect::<Vec<_>>();
    mothers.sort_unstable_by_key(|(_, id, ..)| id.0);
    for (mother, id, pregnancy, reproduction, species, world, transform, habitat, dead) in mothers {
        // Retain the existing removal policy. Death alone does not invalidate
        // the father's identity or the child's lineage.
        if dead
            || !animals
                .get(pregnancy.father)
                .is_ok_and(|father_world| father_world.root == world.root)
        {
            commands.entity(mother).remove::<Pregnancy>();
            continue;
        }
        if pregnancy.due_tick > clock.tick || !identifiers.owns_world(world.root) {
            continue;
        }
        let [minimum, maximum] = reproduction.litter;
        if minimum == 0 || minimum > maximum {
            // Missing reproduction policy must not silently become a default litter.
            continue;
        }
        let Some(species_index) = species_index.get(&species_assets) else {
            continue;
        };
        let Some(definitions) = definitions.get(&definition_assets) else {
            continue;
        };
        let Some(timing) = definitions.simulation_timing() else {
            continue;
        };
        let Some(species) = species_index.find(species.species) else {
            continue;
        };
        let mut random = DeterministicRng::from_entity(
            ZooSeed(seed.0 ^ pregnancy.due_tick.rotate_left(17)),
            *id,
            RngDomain::Reproduction,
        );
        let count = u32::from(minimum)
            + random
                .range_u32(u32::from(maximum - minimum) + 1)
                .expect("validated litter range is nonempty");
        if identifiers.next().checked_add(u64::from(count)).is_none() {
            continue;
        }
        // Resolve the entire litter before changing pregnancy, allocating IDs or
        // emitting outcomes. A missing young variant can then retry losslessly.
        let Some(children) = (0..count)
            .map(|_| {
                resolve_species_and_variant_to_animal_spawn_components(
                    species_index,
                    species,
                    None,
                    None,
                    LifeStage::Young,
                    timing.ticks_per_day,
                    &mut random,
                )
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        for child_components in children {
            let child_id = identifiers
                .allocate(world.root)
                .expect("world and entire litter identifier capacity were validated");
            let child = spawn_animal_entity_from_resolved_components(
                &mut commands,
                species.id,
                child_components,
                child_id,
                *world,
                habitat.map(|membership| membership.habitat_entity),
                Transform::from_translation(transform.translation)
                    .with_rotation(transform.rotation),
                Parents {
                    mother: Some(mother),
                    father: Some(pregnancy.father),
                },
                clock.tick,
            );
            births.write(AnimalBorn { child });
        }
        commands.entity(mother).remove::<Pregnancy>();
    }
}

#[cfg(test)]
mod tests;

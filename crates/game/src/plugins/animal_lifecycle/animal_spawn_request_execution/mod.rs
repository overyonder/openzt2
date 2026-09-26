use bevy::prelude::*;

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::habitat::habitat_types::Habitat;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    animal_entity_spawning::spawn_animal_entity_from_resolved_components,
    animal_habitat_placement_validation::AnimalAquaticHabitatQueries,
    animal_spawn_operation_contracts::{AnimalSpawnRejected, AnimalSpawned, SpawnAnimalRequest},
    animal_spawn_resolution::resolve_animal_spawn_request_to_identifiers_and_components,
};

pub(super) fn spawn_animals_requested_by_gameplay_operations(
    mut commands: Commands,
    mut requests: MessageReader<SpawnAnimalRequest>,
    zoo_seed: Res<ZooSeed>,
    zoo_clock: Res<ZooClock>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut persistent_identifiers: ResMut<PersistentIdAllocator>,
    habitats: Query<&WorldMember, With<Habitat>>,
    aquatic_habitats: AnimalAquaticHabitatQueries,
    mut spawned_animals: MessageWriter<AnimalSpawned>,
    mut rejected_spawns: MessageWriter<AnimalSpawnRejected>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(species_index) = species_index.get(&species_assets) else {
        return;
    };
    for request in requests.read() {
        let (persistent_identifier, world_membership, resolved_components) =
            match resolve_animal_spawn_request_to_identifiers_and_components(
                request,
                &zoo_seed,
                species_index,
                world_definitions,
                &mut persistent_identifiers,
                &habitats,
                &aquatic_habitats,
            ) {
                Ok(resolved_spawn) => resolved_spawn,
                Err(_) => {
                    rejected_spawns.write(AnimalSpawnRejected {
                        operation: request.operation,
                    });
                    continue;
                }
            };
        let animal = spawn_animal_entity_from_resolved_components(
            &mut commands,
            request.species,
            resolved_components,
            persistent_identifier,
            world_membership,
            request.habitat,
            request.transform,
            request.parents,
            zoo_clock.tick,
        );
        spawned_animals.write(AnimalSpawned {
            operation: request.operation,
            animal,
        });
    }
}

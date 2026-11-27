use bevy::prelude::*;
use openzt2_animals::animal_lifecycle::scripted_animal_spawning::{
    resolve_scripted_animal_spawn, ScriptedAnimalSpawnRequest,
};
use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_progression::progression::AwardProgramEffect;
use openzt2_simulation_time::simulation_time_world_lifecycle::zoo_day_cadence_from_world_definitions;
use openzt2_world::world_spawn::prefab_world_instance_spawning::award_crate_delivery::{
    DeliverCratedAwardAnimal, PlaceAwardCratedObject,
};
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        simulation_clock_types::ZooClock,
    },
    world_spawn::world_entity_crating::AnimalCrate,
};

pub(crate) struct AwardWorldEffectsPlugin;

impl Plugin for AwardWorldEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                route_award_crate_deliveries.run_if(on_message::<AwardProgramEffect>),
                deliver_award_animals.run_if(on_message::<DeliverCratedAwardAnimal>),
            ),
        );
    }
}

fn route_award_crate_deliveries(
    mut effects: MessageReader<AwardProgramEffect>,
    mut crates: MessageWriter<PlaceAwardCratedObject>,
) {
    for effect in effects.read() {
        if let AwardProgramEffect::PlaceCratedObject { root, definition } = *effect {
            crates.write(PlaceAwardCratedObject { root, definition });
        }
    }
}

fn deliver_award_animals(
    mut requests: MessageReader<DeliverCratedAwardAnimal>,
    mut spawned: MessageWriter<ScriptedAnimalSpawnRequest>,
    mut commands: Commands,
    species: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    worlds: Query<(&ZooClock, &ZooSeed)>,
) {
    let view = species.get(&species_assets);
    let cadence = definitions
        .get(&definition_assets)
        .and_then(zoo_day_cadence_from_world_definitions);
    for request in requests.read() {
        let Some((view, cadence, (clock, seed))) = view.zip(cadence).and_then(|(view, cadence)| {
            worlds
                .get(request.member.root)
                .ok()
                .map(|world| (view, cadence, world))
        }) else {
            commands.entity(request.crate_entity).try_despawn();
            continue;
        };
        let mut random =
            DeterministicRng::from_entity(*seed, request.persistent_id, RngDomain::AnimalNeeds);
        let Some((species, components)) = resolve_scripted_animal_spawn(
            view,
            request.definition,
            cadence.ticks_per_zoo_day(),
            &mut random,
        ) else {
            commands.entity(request.crate_entity).try_despawn();
            continue;
        };
        let entity = commands
            .spawn((request.member, AnimalCrate(request.crate_entity)))
            .id();
        spawned.write(ScriptedAnimalSpawnRequest {
            entity,
            species,
            components,
            persistent_id: request.persistent_id,
            world: request.member,
            transform: request.pose,
            born_tick: clock.tick,
            initial_needs: Vec::new(),
            crated: false,
            name: None,
        });
    }
}

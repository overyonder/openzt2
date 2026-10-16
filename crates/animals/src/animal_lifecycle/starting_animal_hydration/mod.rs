use super::{
    animal_entity_spawning::insert_animal_entity_from_resolved_components,
    scripted_animal_spawning::{resolve_scripted_animal_spawn, ScriptedAnimalInitialization},
};
use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        simulation_clock_types::ZooClock,
    },
    world_spawn::{
        persistent_id_types::PersistentId, starting_animal_hydration::PendingStartingAnimal,
        world_entity_crating::WorldEntityIsCrated, world_hydration_types::WorldHydration,
        world_load_failure::WorldLoadFailure, world_membership_types::WorldMember,
    },
};
use openzt2_zoo_state::{
    animal_lifecycle::types::{Animal, Parents},
    animal_welfare::types::AnimalWelfare,
};

pub(super) fn hydrate_starting_animals(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    seed: Option<Res<ZooSeed>>,
    clock: Option<Res<ZooClock>>,
    pending: Query<
        (
            Entity,
            &PendingStartingAnimal,
            &PersistentId,
            &WorldMember,
            &Transform,
            Has<WorldEntityIsCrated>,
        ),
        Without<Animal>,
    >,
    mut worlds: Query<&mut WorldHydration>,
) {
    let (Some(seed), Some(clock), Some(species), Some(definitions)) = (
        seed,
        clock,
        species_index.get(&species_assets),
        definitions.get(&definition_assets),
    ) else {
        return;
    };
    for (entity, saved, id, world, transform, crated) in &pending {
        let Ok(mut hydration) = worlds.get_mut(world.root) else {
            continue;
        };
        let mut random = DeterministicRng::from_entity(*seed, *id, RngDomain::AnimalNeeds);
        let Some((species, mut components)) = resolve_scripted_animal_spawn(
            species,
            saved.variant,
            definitions.timing().ticks_per_day,
            &mut random,
        ) else {
            hydration.record_failure(WorldLoadFailure::MissingDependency(saved.variant));
            continue;
        };
        // Saved presentation scale already includes the authored variant scale.
        components.scale = 1.0;
        insert_animal_entity_from_resolved_components(
            &mut commands,
            entity,
            species,
            components,
            *id,
            *world,
            None,
            *transform,
            Parents::NONE,
            clock.tick,
        );
        commands
            .entity(entity)
            .insert(ScriptedAnimalInitialization {
                needs: saved.initial_needs.clone(),
            });
        if crated {
            commands.entity(entity).insert(Visibility::Hidden);
        }
    }
}

pub(super) fn finish_starting_animal_hydration(
    mut commands: Commands,
    animals: Query<
        Entity,
        (
            With<PendingStartingAnimal>,
            With<Animal>,
            With<AnimalWelfare>,
        ),
    >,
) {
    for animal in &animals {
        commands.entity(animal).remove::<PendingStartingAnimal>();
    }
}

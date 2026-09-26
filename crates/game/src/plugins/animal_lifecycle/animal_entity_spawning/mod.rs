use bevy::prelude::*;
use openzt2_game_data::{
    species::{SpeciesFlags, SpeciesVariantFlags},
    AssetId,
};

use crate::plugins::{
    extinct_animals::extinct_animal_observable_fact_types::AuthoredSuperExtinctAnimal,
    habitat::habitat_types::{HabitatLocatable, HabitatMember},
    information::entity_selection_types::Inspectable,
    world_spawn::{
        persistent_id_types::PersistentId, world_membership_types::DefinitionId,
        world_membership_types::WorldMember,
    },
};

use super::{
    animal_spawn_resolution_types::ResolvedAnimalSpawnComponents,
    types::{
        Adoptable, Age, Animal, AnimalLifeStage, AnimalSex, AnimalVariant, EndangeredSpecies,
        Parents, Predator, Prey, SpeciesHandle, Sterile, Swims,
    },
};

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_animal_entity_from_resolved_components(
    commands: &mut Commands,
    species: AssetId,
    resolved_components: ResolvedAnimalSpawnComponents,
    persistent_identifier: PersistentId,
    world_membership: WorldMember,
    habitat_entity: Entity,
    mut transform: Transform,
    parents: Parents,
    born_tick: u64,
) -> Entity {
    transform.scale *= resolved_components.scale;
    let animal_entity = commands
        .spawn((
            Animal,
            SpeciesHandle { species },
            AnimalVariant(resolved_components.variant),
            AnimalSex(resolved_components.sex),
            AnimalLifeStage(resolved_components.life_stage),
            DefinitionId(resolved_components.definition),
            Inspectable {
                definition: resolved_components.definition,
            },
            resolved_components.life_stage_start_ticks,
            resolved_components.reproduction,
            resolved_components.waste,
            resolved_components.presentation_model,
        ))
        .insert((
            Age { born_tick },
            parents,
            persistent_identifier,
            world_membership,
            HabitatMember { habitat_entity },
            HabitatLocatable,
            transform,
            Visibility::Inherited,
        ))
        .id();
    let mut animal_entity_commands = commands.entity(animal_entity);
    insert_authored_species_classification_markers(
        &mut animal_entity_commands,
        resolved_components.species_flags,
    );
    insert_authored_variant_classification_markers(
        &mut animal_entity_commands,
        resolved_components.variant_flags,
    );
    animal_entity
}

fn insert_authored_species_classification_markers(
    animal_entity_commands: &mut EntityCommands,
    species_flags: SpeciesFlags,
) {
    if species_flags.contains(SpeciesFlags::SWIMS) {
        animal_entity_commands.insert(Swims);
    }
    if species_flags.contains(SpeciesFlags::PREDATOR) {
        animal_entity_commands.insert(Predator);
    }
    if species_flags.contains(SpeciesFlags::PREY) {
        animal_entity_commands.insert(Prey);
    }
    if species_flags.contains(SpeciesFlags::ADOPTABLE) {
        animal_entity_commands.insert(Adoptable);
    }
    if species_flags.contains(SpeciesFlags::ENDANGERED) {
        animal_entity_commands.insert(EndangeredSpecies);
    }
    if species_flags.contains(SpeciesFlags::SUPER) {
        animal_entity_commands.insert(AuthoredSuperExtinctAnimal);
    }
}

fn insert_authored_variant_classification_markers(
    animal_entity_commands: &mut EntityCommands,
    variant_flags: SpeciesVariantFlags,
) {
    if variant_flags.contains(SpeciesVariantFlags::STERILE) {
        animal_entity_commands.insert(Sterile);
    }
}

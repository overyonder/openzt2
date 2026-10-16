use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_game_data::{
    species::{LifeStage, NeedKind, Sex},
    AssetId,
};
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::{
        persistent_id_types::PersistentId,
        world_membership_types::{WorldMember, WorldRoot},
    },
};

use super::{
    animal_entity_spawning::insert_animal_entity_from_resolved_components,
    animal_spawn_resolution::{
        lower_species_and_variant_to_animal_spawn_components,
        resolve_species_and_variant_to_animal_spawn_components,
    },
    animal_spawn_resolution_types::ResolvedAnimalSpawnComponents,
    types::{Animal, Parents},
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptedAnimalSpawnSet;

/// Fully resolved scripted creation, retaining the identity returned to the script.
#[derive(Message, Clone)]
pub struct ScriptedAnimalSpawnRequest {
    pub entity: Entity,
    pub species: AssetId,
    pub components: ResolvedAnimalSpawnComponents,
    pub persistent_id: PersistentId,
    pub world: WorldMember,
    pub transform: Transform,
    pub born_tick: u64,
    pub initial_needs: Vec<(NeedKind, i32)>,
    pub crated: bool,
    pub name: Option<String>,
}

/// Initial edits remain attached to the reserved animal until its need rows load.
#[derive(Component)]
pub(crate) struct ScriptedAnimalInitialization {
    pub needs: Vec<(NeedKind, i32)>,
}

pub fn resolve_scripted_animal_spawn(
    species_view: SpeciesView<'_>,
    requested_type: AssetId,
    ticks_per_day: u32,
    random: &mut DeterministicRng,
) -> Option<(AssetId, ResolvedAnimalSpawnComponents)> {
    if let Some((species, variant)) = species_view.find_variant_with_species(requested_type) {
        if variant.sex == Sex::Any {
            let components = resolve_species_and_variant_to_animal_spawn_components(
                species_view,
                species,
                None,
                None,
                variant.life_stage,
                ticks_per_day,
                random,
            )?;
            return Some((species.id, components));
        }
        let components = lower_species_and_variant_to_animal_spawn_components(
            species,
            variant,
            variant.sex,
            variant.life_stage,
            ticks_per_day,
            random,
        )?;
        return Some((species.id, components));
    }
    let species = species_view.find(requested_type)?;
    let components = resolve_species_and_variant_to_animal_spawn_components(
        species_view,
        species,
        None,
        None,
        LifeStage::Adult,
        ticks_per_day,
        random,
    )?;
    Some((species.id, components))
}

pub(super) fn spawn_scripted_animals(
    mut commands: Commands,
    mut requests: MessageReader<ScriptedAnimalSpawnRequest>,
    reserved: Query<&WorldMember, Without<Animal>>,
    worlds: Query<(), With<WorldRoot>>,
) {
    for request in requests.read() {
        if worlds.get(request.world.root).is_err()
            || !reserved
                .get(request.entity)
                .is_ok_and(|world| world.root == request.world.root)
        {
            warn!(animal = ?request.entity, "scripted animal reservation or world no longer exists");
            continue;
        }
        insert_animal_entity_from_resolved_components(
            &mut commands,
            request.entity,
            request.species,
            request.components,
            request.persistent_id,
            request.world,
            None,
            request.transform,
            Parents::NONE,
            request.born_tick,
        );
        commands
            .entity(request.entity)
            .insert(ScriptedAnimalInitialization {
                needs: request.initial_needs.clone(),
            });
        if request.crated {
            commands.entity(request.entity).insert(
                openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated,
            );
        }
        if let Some(name) = &request.name {
            commands
                .entity(request.entity)
                .insert(Name::new(name.clone()));
        }
    }
}

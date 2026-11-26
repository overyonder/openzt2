use bevy::{ecs::world::CommandQueue, prelude::*};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets, SpeciesView};
use openzt2_game_data::{
    AssetId,
    species::{LifeStage, NeedKind},
};
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::{DeterministicRng, ZooSeed},
    world_spawn::{
        persistent_id_types::PersistentId,
        world_membership_types::{WorldMember, WorldRoot},
    },
};

use super::{
    animal_entity_spawning::insert_animal_entity_from_resolved_components,
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
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
    ticks_per_day: impl Into<f64>,
    random: &mut DeterministicRng,
) -> Option<(AssetId, ResolvedAnimalSpawnComponents)> {
    let (species, variant, stage) =
        if let Some((species, variant)) = species_view.find_variant_with_species(requested_type) {
            (species, Some(variant.id), variant.life_stage)
        } else {
            (species_view.find(requested_type)?, None, LifeStage::Adult)
        };
    let components = resolve_species_and_variant_to_animal_spawn_components(
        species_view,
        species,
        variant,
        None,
        stage,
        ticks_per_day.into(),
        random,
    )?;
    Some((species.id, components))
}

pub(super) fn spawn_scripted_animals(
    mut commands: Commands,
    mut requests: MessageReader<ScriptedAnimalSpawnRequest>,
    reserved: Query<&WorldMember, Without<Animal>>,
    worlds: Query<(), With<WorldRoot>>,
    seeds: Query<&ZooSeed>,
    assets: Option<Res<Assets<SpeciesAsset>>>,
    index: Option<Res<SpeciesAssets>>,
) {
    let view = index
        .as_deref()
        .zip(assets.as_deref())
        .and_then(|(index, assets)| index.get(assets));
    for request in requests.read() {
        if worlds.get(request.world.root).is_err()
            || !reserved
                .get(request.entity)
                .is_ok_and(|world| world.root == request.world.root)
        {
            warn!(animal = ?request.entity, "scripted animal reservation or world no longer exists");
            continue;
        }
        let initialized =
            view.zip(seeds.get(request.world.root).ok())
                .is_some_and(|(view, seed)| {
                    initialize_scripted_animal_needs(&mut commands, request, view, *seed)
                });
        queue_scripted_animal_spawn(&mut commands, request);
        if !initialized {
            commands
                .entity(request.entity)
                .insert(ScriptedAnimalInitialization {
                    needs: request.initial_needs.clone(),
                });
        }
    }
}

/// Converts a reserved identity into an animal with its initial authored facts.
pub fn apply_scripted_animal_spawn(
    world: &mut World,
    request: &ScriptedAnimalSpawnRequest,
) -> bool {
    if world.get::<WorldRoot>(request.world.root).is_none()
        || world.get::<Animal>(request.entity).is_some()
        || world.get::<WorldMember>(request.entity) != Some(&request.world)
    {
        return false;
    }
    let Some((view, &seed)) = world
        .get_resource::<SpeciesAssets>()
        .zip(world.get_resource::<Assets<SpeciesAsset>>())
        .and_then(|(index, assets)| index.get(assets))
        .zip(world.get::<ZooSeed>(request.world.root))
    else {
        return false;
    };
    let mut queue = CommandQueue::default();
    {
        let mut commands = Commands::new(&mut queue, world);
        if !initialize_scripted_animal_needs(&mut commands, request, view, seed) {
            return false;
        }
        queue_scripted_animal_spawn(&mut commands, request);
    }
    queue.apply(world);
    true
}

fn initialize_scripted_animal_needs(
    commands: &mut Commands,
    request: &ScriptedAnimalSpawnRequest,
    view: SpeciesView<'_>,
    seed: ZooSeed,
) -> bool {
    crate::animal_welfare::systems::initialize_one_animal_needs(
        &mut commands.entity(request.entity),
        seed,
        request.persistent_id,
        view,
        request.species,
        &request.initial_needs,
    )
}

fn queue_scripted_animal_spawn(commands: &mut Commands, request: &ScriptedAnimalSpawnRequest) {
    insert_animal_entity_from_resolved_components(
        commands,
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
    if request.crated {
        commands
            .entity(request.entity)
            .insert(openzt2_world_state::world_spawn::world_entity_crating::ScriptedCrating);
    }
    if let Some(name) = &request.name {
        commands
            .entity(request.entity)
            .insert(Name::new(name.clone()));
    }
}

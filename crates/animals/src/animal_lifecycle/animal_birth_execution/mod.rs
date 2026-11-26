use avian3d::prelude::RigidBody;
use bevy::{asset::LoadState, prelude::*};
#[cfg(test)]
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
#[cfg(test)]
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_game_data::species::LifeStage;
use openzt2_world::world_spawn::prefab_world_instance_spawning::{
    morph_fade_projection::PrefabMorphFade, spawn_loaded_scene_prefab_as_world_instance,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorSpawnChild, RunningBehaviorInstruction,
};

use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::animal_lineage::{
    AnimalMate, PregnancyFather, insert_animal_parentage,
};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::{
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatIndex, HabitatMember},
    },
    topology::topology_graph_types::TopologyGrid,
};

use super::{
    animal_birth_and_release_contracts::AnimalBorn,
    animal_entity_spawning::spawn_animal_entity_from_resolved_components,
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
    types::{Animal, Parents, SpeciesHandle},
};

/// Authored births: each `SpawnChild` action, played by the `Birth` set
/// (`SpawnF` and `SpawnM`, one to three times), brings one child of its
/// young variant beside the mother.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_children_for_authored_births(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut identifiers: Query<&mut PersistentIdAllocator>,
    prefabs: Option<Res<Assets<ScenePrefabAsset>>>,
    server: Option<Res<AssetServer>>,
    habitat_grid: Query<(&HabitatIndex, &TopologyGrid)>,
    mut mothers: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorSpawnChild,
            ),
            &mut BehaviorRandomStream,
            &SpeciesHandle,
            &WorldMember,
            &Transform,
            Option<&HabitatMember>,
            Option<&PregnancyFather>,
            Option<&AnimalMate>,
        ),
        (With<Animal>, Without<PendingBehaviorTaskFailure>),
    >,
    mut births: MessageWriter<AnimalBorn>,
) {
    let (Some(species_index), Some(view)) = (
        species_index.get(&species_assets),
        definitions.get(&definition_assets),
    ) else {
        return;
    };
    for (
        mother,
        (task, instruction, running_action),
        mut random,
        species,
        world,
        transform,
        habitat,
        pregnancy,
        mate,
    ) in
        openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors(
            &mut mothers,
            activation.0,
        )
    {
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let &RunBehaviorSpawnChild {
            entity,
            pose,
            fade_in,
            within_habitat,
        } = running_action;
        let constrain = |mut position: Transform| {
            if within_habitat {
                if let Some(((habitats, topology), habitat)) =
                    habitat_grid.get(world.root).ok().zip(habitat)
                {
                    if locate_habitat_at_world_position(
                        habitats,
                        position.translation.xz(),
                        *topology,
                    ) != Some(habitat.habitat_entity)
                    {
                        position.translation = transform.translation;
                    }
                }
            }
            position
        };
        let parents = Parents {
            mother: Some(mother),
            father: pregnancy.map(|father| father.0).or(mate.map(|mate| mate.0)),
        };
        if let Some(egg) = view
            .find_object(entity)
            .filter(|object| object.authored_runtime.egg.is_some())
        {
            let Some(handle) = view.scene(egg.prefab) else {
                if definitions.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
                }
                continue;
            };
            let Some(prefab) = prefabs.as_deref().and_then(|prefabs| prefabs.get(&handle)) else {
                if server.as_deref().is_some_and(|server| {
                    matches!(
                        server.get_load_state(handle.id()),
                        Some(LoadState::Failed(_))
                    )
                }) {
                    mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
                }
                continue;
            };
            let Some(child_id) = identifiers
                .get_mut(world.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
                continue;
            };
            let place = Transform {
                scale: Vec3::splat(egg.prefab_scale),
                ..constrain(birth_pose(transform, pose, &mut random.0))
            };
            let child = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                handle,
                world.root,
                egg.id,
                child_id,
                place,
                true,
                None,
                RigidBody::Kinematic,
            );
            let mut child_commands = commands.entity(child);
            insert_animal_parentage(&mut child_commands, parents);
            if !fade_in.is_zero() {
                child_commands.insert(PrefabMorphFade::fade_in(fade_in));
            }
            if let Some(habitat) = habitat {
                child_commands.insert(*habitat);
            }
            instruction.complete(mother, clock.tick, &mut commands);
            continue;
        }
        if view.simulation_timing().is_none() {
            continue;
        }
        let child = species_index.find(species.species).and_then(|definition| {
            resolve_species_and_variant_to_animal_spawn_components(
                species_index,
                definition,
                Some(entity),
                None,
                LifeStage::Young,
                crate::zoo_day_cadence(view).ticks_per_zoo_day(),
                &mut random.0,
            )
            .map(|components| (definition.id, components))
        });
        let Some((species_id, components)) = child else {
            mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
            continue;
        };
        let Some(child_id) = identifiers
            .get_mut(world.root)
            .ok()
            .and_then(|mut allocator| allocator.allocate().ok())
        else {
            mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
            continue;
        };
        let place = constrain(birth_pose(transform, pose, &mut random.0));
        let child = spawn_animal_entity_from_resolved_components(
            &mut commands,
            species_id,
            components,
            child_id,
            *world,
            habitat.map(|membership| membership.habitat_entity),
            place,
            parents,
            clock.tick,
        );
        if !fade_in.is_zero() {
            commands
                .entity(child)
                .insert(PrefabMorphFade::fade_in(fade_in));
        }
        births.write(AnimalBorn { child });
        instruction.complete(mother, clock.tick, &mut commands);
    }
}

fn birth_pose(
    transform: &Transform,
    pose: openzt2_game_data::behavior::action::spawn::BehaviorSpawnPosePolicy,
    random: &mut openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng,
) -> Transform {
    let mut offset = |range: f32| (random.unit_f32() * 2.0 - 1.0) * range;
    let mut place = Transform::from_translation(transform.translation);
    place.translation.x += offset(pose.random_position_offset_m);
    place.translation.z += offset(pose.random_position_offset_m);
    place.rotation =
        transform.rotation * Quat::from_rotation_y(offset(pose.random_rotation_offset_radians));
    place
}

#[cfg(test)]
mod tests;

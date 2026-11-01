use bevy::prelude::*;
use openzt2_game_data::{behavior::action_record::BehaviorAction, species::LifeStage};

use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;

use super::{
    animal_birth_and_release_contracts::AnimalBorn,
    animal_entity_spawning::spawn_animal_entity_from_resolved_components,
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
    types::{Animal, Mate, Parents, Pregnancy, SpeciesHandle},
};

/// Authored births: each `SpawnChild` action, played by the `Birth` set
/// (`SpawnF` and `SpawnM`, one to three times), brings one child of its
/// young variant beside the mother.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_children_for_authored_births(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut identifiers: ResMut<PersistentIdAllocator>,
    mut mothers: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &mut BehaviorRandomStream,
            &SpeciesHandle,
            &WorldMember,
            &Transform,
            Option<&HabitatMember>,
            Option<&Pregnancy>,
            Option<&Mate>,
        ),
        (With<Animal>, Without<PendingBehaviorTaskFailure>),
    >,
    mut births: MessageWriter<AnimalBorn>,
) {
    let (Some(species_index), Some(timing)) = (
        species_index.get(&species_assets),
        definitions
            .get(&definition_assets)
            .and_then(|definitions| definitions.simulation_timing()),
    ) else {
        return;
    };
    for (mother, mut task, mut random, species, world, transform, habitat, pregnancy, mate) in
        &mut mothers
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::SpawnChild { entity, pose }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
            .cloned()
        else {
            continue;
        };
        let child = species_index.find(species.species).and_then(|definition| {
            resolve_species_and_variant_to_animal_spawn_components(
                species_index,
                definition,
                Some(entity),
                None,
                LifeStage::Young,
                timing.ticks_per_day,
                &mut random.0,
            )
            .map(|components| (definition.id, components))
        });
        let Some((species_id, components)) = child else {
            mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
            continue;
        };
        let Ok(child_id) = identifiers.allocate(world.root) else {
            mark_behavior_task_for_failure_and_stop_navigation(mother, &mut commands);
            continue;
        };
        let mut offset = |range: f32| (random.0.unit_f32() * 2.0 - 1.0) * range;
        let mut place = Transform::from_translation(transform.translation);
        place.translation.x += offset(pose.random_position_offset_m);
        place.translation.z += offset(pose.random_position_offset_m);
        place.rotation =
            transform.rotation * Quat::from_rotation_y(offset(pose.random_rotation_offset_radians));
        let child = spawn_animal_entity_from_resolved_components(
            &mut commands,
            species_id,
            components,
            child_id,
            *world,
            habitat.map(|membership| membership.habitat_entity),
            place,
            Parents {
                mother: Some(mother),
                father: pregnancy
                    .and_then(|pregnancy| pregnancy.father)
                    .or(mate.map(|mate| mate.0)),
            },
            clock.tick,
        );
        births.write(AnimalBorn { child });
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

#[cfg(test)]
mod tests;

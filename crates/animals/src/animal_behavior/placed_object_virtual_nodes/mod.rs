//! Gives a placed object or an animal its authored `virtualNodes` as named
//! children, so a building's `Dock_Rampage1` or a dinosaur's `Dock_Building`
//! resolves like a model node.

use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::world_definitions::world_objects::WorldObjectVirtualNode;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

/// Marks an object whose virtual nodes have been spawned.
#[derive(Component)]
struct VirtualNodesSpawned;

pub(super) fn register_placed_object_virtual_nodes(application: &mut App) {
    application.add_systems(
        Update,
        (
            spawn_placed_object_virtual_nodes,
            spawn_animal_virtual_nodes,
        )
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn spawn_placed_object_virtual_nodes(
    mut commands: Commands,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    // Placed objects, guests and staff all carry their definition.
    objects: Query<(Entity, &DefinitionId), (Without<VirtualNodesSpawned>, Without<AnimalVariant>)>,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (entity, reference) in &objects {
        commands.entity(entity).insert(VirtualNodesSpawned);
        let Some(object) = definitions.find_object(reference.0) else {
            continue;
        };
        spawn_virtual_nodes(&mut commands, entity, &object.virtual_nodes);
    }
}

fn spawn_animal_virtual_nodes(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    animals: Query<(Entity, &AnimalVariant), Without<VirtualNodesSpawned>>,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    for (entity, variant) in &animals {
        commands.entity(entity).insert(VirtualNodesSpawned);
        if let Some(variant) = species.find_variant(variant.0) {
            spawn_virtual_nodes(&mut commands, entity, &variant.virtual_nodes);
        }
    }
}

fn spawn_virtual_nodes(commands: &mut Commands, parent: Entity, nodes: &[WorldObjectVirtualNode]) {
    for node in nodes {
        commands.spawn((
            Name::new(node.name.clone()),
            Transform::from_translation(Vec3::from_array(node.translation_m))
                .with_rotation(Quat::from_rotation_y(node.yaw_degrees.to_radians())),
            ChildOf(parent),
        ));
    }
}

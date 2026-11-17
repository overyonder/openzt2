//! Gives a placed object its authored `virtualNodes` as named children, so a
//! slot's `targetNode` (a building's `Dock_Rampage1`) resolves like a model node.

use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;

/// Marks an object whose virtual nodes have been spawned.
#[derive(Component)]
struct VirtualNodesSpawned;

pub(super) fn register_placed_object_virtual_nodes(application: &mut App) {
    application.add_systems(
        Update,
        spawn_placed_object_virtual_nodes.run_if(in_state(GamePhase::InGame)),
    );
}

fn spawn_placed_object_virtual_nodes(
    mut commands: Commands,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    objects: Query<(Entity, &PlacedObjectDefinitionReference), Without<VirtualNodesSpawned>>,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (entity, reference) in &objects {
        commands.entity(entity).insert(VirtualNodesSpawned);
        let Some(object) = definitions.find_object(reference.definition) else {
            continue;
        };
        for node in &object.virtual_nodes {
            // ponytail: yaw taken as a turn about Bevy's up axis; only docking
            // positions are checked against the original so far.
            commands.spawn((
                Name::new(node.name.clone()),
                Transform::from_translation(Vec3::from_array(node.translation_m))
                    .with_rotation(Quat::from_rotation_y(node.yaw_degrees.to_radians())),
                ChildOf(entity),
            ));
        }
    }
}

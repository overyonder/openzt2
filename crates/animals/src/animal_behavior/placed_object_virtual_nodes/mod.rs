//! Projects authored virtual nodes as named render-tree attachments.

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::world_definitions::world_objects::WorldObjectVirtualNode;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

#[derive(Component)]
#[relationship(relationship_target = VirtualNodes)]
struct VirtualNodeOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = VirtualNodeOf)]
struct VirtualNodes(Vec<Entity>);

pub(super) fn register_placed_object_virtual_nodes(app: &mut App) {
    app.add_systems(
        Update,
        project_virtual_nodes.run_if(in_state(GamePhase::InGame)),
    );
}

fn project_virtual_nodes(
    mut commands: Commands,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world: Res<WorldDefinitions>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species: Res<SpeciesAssets>,
    entities: Query<
        (
            Entity,
            Option<&AnimalVariant>,
            Option<&DefinitionId>,
            Option<&VirtualNodes>,
        ),
        Or<(With<AnimalVariant>, With<DefinitionId>)>,
    >,
    changed: Query<Entity, Or<(Changed<AnimalVariant>, Changed<DefinitionId>)>>,
    mut removed_variants: RemovedComponents<AnimalVariant>,
    mut removed_definitions: RemovedComponents<DefinitionId>,
    existing_nodes: Query<(&Name, &Transform), With<VirtualNodeOf>>,
) {
    let all = world.is_changed()
        || world_assets.is_changed()
        || species.is_changed()
        || species_assets.is_changed();
    let mut affected: Vec<_> = if all {
        entities.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed.iter().collect()
    };
    affected.extend(removed_variants.read());
    affected.extend(removed_definitions.read());
    affected.sort_unstable();
    affected.dedup();
    let world = world.get(&world_assets);
    let species = species.get(&species_assets);
    for entity in affected {
        let Ok((_, variant, definition, current)) = entities.get(entity) else {
            commands.queue(move |world: &mut World| {
                if let Ok(mut entity) = world.get_entity_mut(entity) {
                    entity.despawn_related::<VirtualNodes>();
                }
            });
            continue;
        };
        let nodes = if let Some(variant) = variant {
            species
                .and_then(|species| species.find_variant(variant.0))
                .map(|variant| &variant.virtual_nodes)
        } else {
            definition
                .and_then(|definition| world?.find_object(definition.0))
                .map(|object| &object.virtual_nodes)
        };
        let nodes = nodes.map_or(&[][..], Vec::as_slice);
        let unchanged = current.map_or(nodes.is_empty(), |current| {
            current.0.len() == nodes.len()
                && current.iter().zip(nodes).all(|(child, authored)| {
                    existing_nodes.get(child).is_ok_and(|(name, local)| {
                        name.as_str() == authored.name && *local == virtual_node_transform(authored)
                    })
                })
        });
        if unchanged {
            continue;
        }
        commands.entity(entity).despawn_related::<VirtualNodes>();
        for node in nodes {
            commands.spawn((
                Name::new(node.name.clone()),
                virtual_node_transform(node),
                ChildOf(entity),
                VirtualNodeOf(entity),
            ));
        }
    }
}

fn virtual_node_transform(node: &WorldObjectVirtualNode) -> Transform {
    Transform::from_translation(Vec3::from_array(node.translation_m))
        .with_rotation(Quat::from_rotation_y(node.yaw_degrees.to_radians()))
}

//! Detach-created inventory items reserve the canonical container place before
//! demand-loaded presentation arrives.

use super::*;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::prefab_world_instance_spawning::hydrate_loaded_scene_prefab_into_world_instance_root;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;

#[derive(Component)]
pub(super) struct PendingDetachCreatedObject {
    prefab: Option<Handle<ScenePrefabAsset>>,
}

pub(super) fn reserve_detach_created_objects(
    holder: Entity,
    holder_definition: AssetId,
    world: WorldMember,
    joint_transform: GlobalTransform,
    relative_transform: Transform,
    created: &[(AssetId, WorldObjectDetachDestination)],
    definitions: &WorldDefinitionsView<'_>,
    ids: &mut PersistentIdAllocator,
    occupancy: &mut InteractionContainerOccupancy,
    commands: &mut Commands,
) {
    let Some(holder_policy) = definitions.find_object(holder_definition) else {
        return;
    };
    for &(definition, destination) in created {
        let WorldObjectDetachDestination::Container(destination) = destination else {
            continue;
        };
        let Some((slot, policy)) = holder_policy
            .interaction_slots
            .iter()
            .enumerate()
            .find(|(_, slot)| !slot.is_queue && slot.reservation_tag == destination)
        else {
            continue;
        };
        if occupancy.occupied_places(holder, slot) >= usize::from(policy.capacity) {
            continue;
        }
        let Ok(id) = ids.allocate(world.root) else {
            warn!(
                ?holder,
                "cannot allocate persistent identity for detach-created object"
            );
            continue;
        };
        let item = commands
            .spawn((
                PendingDetachCreatedObject { prefab: None },
                DefinitionId(definition),
                world,
                id,
                joint_transform.compute_transform(),
                joint_transform,
                ContainedObject {
                    holder_definition,
                    detach_rule: AssetId::default(),
                    joint: None,
                    relative_transform,
                    attachment_request: None,
                },
                Visibility::Hidden,
            ))
            .id();
        if !occupancy.admit(holder, item, slot, policy.capacity) {
            commands.entity(item).despawn();
        }
    }
}

pub(super) fn hydrate_detach_created_objects(
    mut commands: Commands,
    assets: AttachmentAssets,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    mut pending: Query<
        (
            Entity,
            &mut PendingDetachCreatedObject,
            &DefinitionId,
            &WorldMember,
            &PersistentId,
            &GlobalTransform,
            Option<&ContainedObject>,
        ),
        With<PendingDetachCreatedObject>,
    >,
    holders: Query<&GlobalTransform>,
) {
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (pending_entity, mut loading, definition, world, id, transform, contained) in &mut pending {
        let membership = occupancy.container_for_member(pending_entity);
        if contained.is_some() && membership.is_none() {
            continue;
        }
        let instance_transform =
            if let (Some((holder, _)), Some(contained)) = (membership, contained) {
                let Ok(holder_transform) = holders.get(holder) else {
                    // Holder cleanup removes owned items or releases unowned ones.
                    continue;
                };
                holder_transform
                    .mul_transform(contained.relative_transform)
                    .compute_transform()
            } else {
                transform.compute_transform()
            };
        let Some(object) = definitions.find_object(definition.0) else {
            if assets.definitions.is_complete() {
                occupancy.remove_actor(pending_entity);
                commands.entity(pending_entity).despawn();
            }
            continue;
        };
        let presentation = if object.prefab == AssetId::default() {
            None
        } else {
            let Some(prefab_handle) = loading
                .prefab
                .clone()
                .or_else(|| definitions.scene(object.prefab))
            else {
                if assets.definitions.is_complete() {
                    occupancy.remove_actor(pending_entity);
                    commands.entity(pending_entity).despawn();
                }
                continue;
            };
            if loading.prefab.is_none() {
                loading.prefab = Some(prefab_handle.clone());
            }
            let Some(prefab) = assets.prefabs.get(&prefab_handle) else {
                if matches!(
                    assets.server.get_load_state(prefab_handle.id()),
                    Some(LoadState::Failed(_))
                ) {
                    occupancy.remove_actor(pending_entity);
                    commands.entity(pending_entity).despawn();
                }
                continue;
            };
            Some((prefab_handle, prefab))
        };
        let policy = if let Some((_, slot)) = membership {
            let Some(policy) = contained
                .and_then(|contained| definitions.find_object(contained.holder_definition))
                .and_then(|holder| holder.interaction_slots.get(slot))
            else {
                continue;
            };
            Some(policy)
        } else {
            None
        };
        let scale = Transform::from_scale(Vec3::splat(object.prefab_scale));
        if let Some((prefab_handle, prefab)) = presentation.as_ref() {
            hydrate_loaded_scene_prefab_into_world_instance_root(
                &mut commands,
                pending_entity,
                prefab,
                prefab_handle.clone(),
                world.root,
                object.id,
                *id,
                instance_transform.mul_transform(scale),
                true,
                None,
                RigidBody::Kinematic,
            );
        } else {
            commands.entity(pending_entity).insert((
                Inspectable {
                    definition: object.id,
                },
                instance_transform.mul_transform(scale),
                Visibility::Inherited,
            ));
        }
        // Hide the inventory root only; descendants retain authored visibility
        // so a later release can reveal the same hydrated hierarchy.
        if policy.is_some_and(|policy| policy.hides_contents) {
            commands.entity(pending_entity).insert(Visibility::Hidden);
        }
        if let Some(contained) = contained {
            commands.entity(pending_entity).insert(ContainedObject {
                holder_definition: contained.holder_definition,
                detach_rule: contained.detach_rule,
                joint: None,
                attachment_request: contained.attachment_request,
                relative_transform: contained
                    .relative_transform
                    .mul_transform(scale)
                    .mul_transform(presentation.as_ref().map_or(
                        Transform::IDENTITY,
                        |(_, prefab)| {
                            transform_from_authored(
                                &prefab.canonical_scene_prefab_document().entities[0].transform,
                            )
                        },
                    )),
            });
        }
        commands
            .entity(pending_entity)
            .remove::<PendingDetachCreatedObject>();
    }
}

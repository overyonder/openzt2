//! Detach-created inventory items reserve the canonical container place before
//! demand-loaded presentation arrives.

use super::*;
use super::admission::{DetachAdmission, DetachMembershipRequest};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::InteractionContainerAdmissionRequest;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world::world_spawn::prefab_world_instance_spawning::hydrate_loaded_scene_prefab_into_world_instance_root;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;

#[derive(Component, Default)]
pub(super) struct PendingDetachCreatedObject {
    prefab: Option<Handle<ScenePrefabAsset>>,
    settled: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reserve_detach_created_objects(
    holder: Entity,
    holder_definition: AssetId,
    world: WorldMember,
    joint_transform: GlobalTransform,
    relative_transform: Transform,
    created: &[(AssetId, WorldObjectDetachDestination)],
    definitions: &WorldDefinitionsView<'_>,
    commands: &mut Commands,
) {
    let holder_policy = super::holder::AttachmentHolder::find(definitions, holder_definition);
    for &(definition, destination) in created {
        let destination = match destination {
            WorldObjectDetachDestination::Container(destination) => destination,
            WorldObjectDetachDestination::Drop | WorldObjectDetachDestination::Fall => {
                let motion = if destination == WorldObjectDetachDestination::Drop {
                    BehaviorWorldReleaseMotion::Drop
                } else {
                    BehaviorWorldReleaseMotion::Fall { speed_mps: 0.0 }
                };
                commands.queue(move |state: &mut World| {
                    let Some(mut ids) = state.get_mut::<openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator>(world.root) else { return; };
                    let Ok(id) = ids.allocate() else { return; };
                    state.spawn((
                        id, world, DefinitionId(definition), joint_transform.compute_transform(),
                        joint_transform, motion, PendingDetachCreatedObject::default(),
                    ));
                });
                continue;
            }
            WorldObjectDetachDestination::Kill => continue,
        };
        let Some(holder_policy) = holder_policy.as_ref() else {
            continue;
        };
        let Some((slot, policy)) = holder_policy
            .place_for(destination)
            .and_then(|slot| Some((slot, holder_policy.place(slot)?)))
        else {
            continue;
        };
        DetachMembershipRequest::Admit(
            InteractionContainerAdmissionRequest::AdmitNew {
                container: holder,
                slot,
                capacity: policy.capacity,
                world_root: world.root,
            },
            DetachAdmission::Create {
                holder,
                definition,
                world,
                transform: joint_transform,
                attachment: ContainedObject {
                    holder_definition,
                    detach_rule: AssetId::default(),
                    joint: None,
                    relative_transform,
                    attachment_request: None,
                },
            },
        )
        .submit(commands);
    }
}

pub(super) fn hydrate_detach_created_objects(
    mut commands: Commands,
    assets: AttachmentAssets,
    mut prefab_events: AttachmentPrefabEvents,
    memberships: InteractionContainerMemberships,
    mut pending: Query<
        (
            Entity,
            &mut PendingDetachCreatedObject,
            &DefinitionId,
            &WorldMember,
            &PersistentId,
            &GlobalTransform,
            Option<&mut ContainedObject>,
            Option<&BehaviorWorldReleaseMotion>,
        ),
        With<PendingDetachCreatedObject>,
    >,
    holders: Query<&GlobalTransform>,
) {
    let settled = prefab_events.settled();
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (pending_entity, mut loading, definition, world, id, transform, contained, motion) in
        &mut pending
    {
        if let Some(prefab) = loading.prefab.as_ref() {
            if !loading.settled {
                if !settled.contains(&prefab.id()) {
                    continue;
                }
                loading.settled = true;
            }
        }
        let membership = memberships.container_for_member(pending_entity);
        if contained.is_some() && membership.is_none() {
            continue;
        }
        let instance_transform =
            if let (Some((holder, _)), Some(contained)) = (membership, contained.as_deref()) {
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
                    commands.entity(pending_entity).despawn();
                }
                continue;
            };
            Some((prefab_handle, prefab))
        };
        let policy = if let Some((_, slot)) = membership {
            let Some(policy) = contained
                .as_deref()
                .and_then(|contained| {
                    super::holder::AttachmentHolder::find(&definitions, contained.holder_definition)
                })
                .and_then(|holder| holder.place(slot))
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
        if let Some(mut contained) = contained {
            let transform = contained
                .relative_transform
                .mul_transform(scale)
                .mul_transform(
                    presentation
                        .as_ref()
                        .map_or(Transform::IDENTITY, |(_, prefab)| {
                            transform_from_authored(
                                &prefab.canonical_scene_prefab_document().entities[0].transform,
                            )
                        }),
                );
            contained
                .reborrow()
                .map_unchanged(|attachment| &mut attachment.joint)
                .set_if_neq(None);
            contained
                .map_unchanged(|attachment| &mut attachment.relative_transform)
                .set_if_neq(transform);
        }
        if let Some(motion) = motion {
            commands.trigger(BehaviorWorldObjectReleased {
                object: pending_entity,
                world: world.root,
                pose: instance_transform.mul_transform(scale),
                motion: *motion,
            });
            commands
                .entity(pending_entity)
                .remove::<BehaviorWorldReleaseMotion>();
        }
        commands
            .entity(pending_entity)
            .remove::<PendingDetachCreatedObject>();
    }
}

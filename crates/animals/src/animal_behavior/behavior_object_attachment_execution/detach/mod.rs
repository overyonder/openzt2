use super::*;
use super::admission::{DetachAdmission, DetachMembershipRequest};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::InteractionContainerAdmissionRequest;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerSlots;

pub(super) fn detach_slot_objects(
    holder: Entity,
    slot: usize,
    definitions: &WorldDefinitionsView<'_>,
    objects: &Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: &Query<&GlobalTransform>,
    memberships: &InteractionContainerMemberships,
    world_releases: Option<&BehaviorWorldReleaseSupport>,
    commands: &mut Commands,
) -> bool {
    memberships.members_in_slot(holder, slot).all(|item| {
        detach_object(
            holder,
            item,
            None,
            definitions,
            objects,
            transforms,
            world_releases,
            commands,
        )
    })
}

/// Applies `rule`, or the rule the item was attached with, to one item.
#[allow(clippy::too_many_arguments)]
pub(super) fn detach_object(
    holder: Entity,
    item: Entity,
    rule: Option<AssetId>,
    definitions: &WorldDefinitionsView<'_>,
    objects: &Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: &Query<&GlobalTransform>,
    world_releases: Option<&BehaviorWorldReleaseSupport>,
    commands: &mut Commands,
) -> bool {
    let Ok((contained, definition, world)) = objects.get(item) else {
        return false;
    };
    let detach_rule = rule.unwrap_or(contained.detach_rule);
    if detach_rule == AssetId::default() {
        DetachMembershipRequest::Release {
            member: item,
            despawn: false,
        }
        .submit(commands);
        commands
            .entity(item)
            .remove::<ContainedObject>()
            .insert(Visibility::Inherited);
        return true;
    }
    let Some(object) = definitions.find_object(definition.0) else {
        return false;
    };
    let Some(rule) = object
        .detach_actions
        .iter()
        .find(|rule| rule.name == detach_rule)
    else {
        DetachMembershipRequest::Release {
            member: item,
            despawn: true,
        }
        .submit(commands);
        return true;
    };
    if rule
        .created_objects
        .iter()
        .any(|(_, destination)| match destination {
            WorldObjectDetachDestination::Container(_) => false,
            WorldObjectDetachDestination::Drop => {
                !BehaviorWorldReleaseMotion::Drop.available(world_releases)
            }
            WorldObjectDetachDestination::Fall => {
                !BehaviorWorldReleaseMotion::Fall.available(world_releases)
            }
            WorldObjectDetachDestination::Kill => true,
        })
        || (!rule.created_objects.is_empty() && world.is_none())
    {
        return false;
    }
    let needs_joint = !rule.created_objects.is_empty()
        || matches!(
            rule.destination,
            WorldObjectDetachDestination::Drop | WorldObjectDetachDestination::Fall
        );
    let prepared_pose = if needs_joint {
        let Ok(joint) = transforms.get(contained.joint.unwrap_or(holder)) else {
            return false;
        };
        Some(*joint)
    } else {
        None
    };
    let created_transform = if rule.created_objects.is_empty() {
        None
    } else {
        let Ok(holder_transform) = transforms.get(holder) else {
            return false;
        };
        let joint = prepared_pose.expect("creation requires a joint");
        Some((joint, joint.reparented_to(holder_transform)))
    };
    let detached = (|| {
        match rule.destination {
            WorldObjectDetachDestination::Kill => {
                DetachMembershipRequest::Release {
                    member: item,
                    despawn: true,
                }
                .submit(commands);
            }
            WorldObjectDetachDestination::Container(destination) => {
                let Some(holder_definition) =
                    super::holder::AttachmentHolder::find(definitions, contained.holder_definition)
                else {
                    return false;
                };
                let Some((target_slot, target)) = holder_definition
                    .place_for(destination)
                    .and_then(|slot| Some((slot, holder_definition.place(slot)?)))
                else {
                    DetachMembershipRequest::Release {
                        member: item,
                        despawn: true,
                    }
                    .submit(commands);
                    return true;
                };
                let attachment = ContainedObject {
                    holder_definition: contained.holder_definition,
                    detach_rule,
                    joint: None,
                    relative_transform: Transform::IDENTITY,
                    attachment_request: None,
                };
                let visibility = if target.hides_contents {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
                DetachMembershipRequest::Admit(
                    InteractionContainerAdmissionRequest::Admit {
                        container: holder,
                        member: item,
                        slots: vec![(target_slot, target.capacity)],
                        despawn_on_refusal: true,
                        queue_service_slots: Vec::new(),
                    },
                    DetachAdmission::Transfer(attachment, visibility),
                )
                .submit(commands);
            }
            WorldObjectDetachDestination::Drop | WorldObjectDetachDestination::Fall => {
                let motion = if rule.destination == WorldObjectDetachDestination::Drop {
                    BehaviorWorldReleaseMotion::Drop
                } else {
                    BehaviorWorldReleaseMotion::Fall
                };
                let Some(world) = world.filter(|_| motion.available(world_releases)) else {
                    return false;
                };
                let pose = prepared_pose
                    .expect("release requires a joint")
                    .mul_transform(contained.relative_transform)
                    .compute_transform();
                DetachMembershipRequest::Release {
                    member: item,
                    despawn: false,
                }
                .submit(commands);
                commands.entity(item).queue_silenced(
                    move |mut item: bevy::ecs::world::EntityWorldMut<'_>| {
                        item.remove::<ContainedObject>();
                        if let Some(mut transform) = item.get_mut::<Transform>() {
                            transform.set_if_neq(pose);
                        } else {
                            item.insert(pose);
                        }
                        if let Some(mut visibility) = item.get_mut::<Visibility>() {
                            visibility.set_if_neq(Visibility::Inherited);
                        } else {
                            item.insert(Visibility::Inherited);
                        }
                    },
                );
                commands.trigger(BehaviorWorldObjectReleased {
                    object: item,
                    world: world.root,
                    pose,
                    motion,
                });
            }
        }
        true
    })();
    if !detached {
        return false;
    }
    if let (Some(world), Some((joint_transform, relative_transform))) = (world, created_transform) {
        created::reserve_detach_created_objects(
            holder,
            contained.holder_definition,
            *world,
            joint_transform,
            relative_transform,
            &rule.created_objects,
            definitions,
            commands,
        );
    }
    true
}

#[derive(Component)]
pub(super) struct DestroyedHolderSlot(pub(super) usize);

pub(super) fn record_slots_of_destroyed_holders(
    despawn: On<Despawn<InteractionContainerSlots>>,
    memberships: InteractionContainerMemberships,
    carried: Query<(), With<ContainedObject>>,
    mut commands: Commands,
) {
    // Slot despawn releases the relationships before the next scheduled system.
    for member in memberships.members_of_container(despawn.entity) {
        if !carried.contains(member) {
            continue;
        }
        if let Some(slot) = memberships.actor_slot(despawn.entity, member) {
            commands
                .entity(member)
                .try_insert(DestroyedHolderSlot(slot));
        }
    }
}

pub(super) fn release_objects_from_destroyed_holders(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    objects: Query<
        (Entity, &ContainedObject, Option<&DestroyedHolderSlot>),
        Without<InteractionContainerMember>,
    >,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (item, contained, destroyed_slot) in &objects {
        let owned = if let Some(slot) = destroyed_slot {
            let Some(policy) =
                super::holder::AttachmentHolder::find(&definitions, contained.holder_definition)
                    .and_then(|holder| holder.place(slot.0))
            else {
                continue;
            };
            policy.owns_contents
        } else {
            false
        };
        if owned {
            commands.entity(item).despawn();
        } else {
            commands
                .entity(item)
                .remove::<(ContainedObject, DestroyedHolderSlot)>()
                .insert(Visibility::Inherited);
        }
    }
}

/// Text-key traversal may cross attach and detach in one simulation step. The
/// first key resolves the joint; this runs after deferred prefab admission, so
/// the later key cannot disappear merely because its item did not exist yet.
pub(super) fn apply_delivered_detach_after_object_admission(
    mut commands: Commands,
    world_releases: Option<Res<BehaviorWorldReleaseSupport>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    delivered: Query<Entity, With<DeliveredAttachmentDetach>>,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: Query<&GlobalTransform>,
    memberships: InteractionContainerMemberships,
    mut delivered_slots: Local<Vec<(Entity, usize)>>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    // Snapshot and coalesce before any rule transfers members or creates new
    // ones. A later marked member must not detach a newly populated destination.
    // Preserve query order so cross-holder creation keeps deterministic IDs.
    delivered_slots.clear();
    for item in &delivered {
        commands.entity(item).remove::<DeliveredAttachmentDetach>();
        if let Some(slot) = memberships.container_for_member(item) {
            if !delivered_slots.contains(&slot) {
                delivered_slots.push(slot);
            }
        }
    }
    for &(holder, slot) in delivered_slots.iter() {
        if !detach_slot_objects(
            holder,
            slot,
            &definitions,
            &objects,
            &transforms,
            &memberships,
            world_releases.as_deref(),
            &mut commands,
        ) {
            mark_behavior_task_for_failure_and_stop_navigation(holder, &mut commands);
        }
    }
}

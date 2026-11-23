use super::*;
use super::admission::{DetachAdmission, DetachMembershipRequest};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::InteractionContainerAdmissionRequest;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerSlots;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::CarriedPrefabPresentation;

#[derive(EntityEvent)]
pub(super) struct FinishObjectRelease {
    #[event_target]
    item: Entity,
    motion: Option<BehaviorWorldObjectReleased>,
}

pub(super) fn detach_slot_objects(
    holder: Entity,
    slot: usize,
    definitions: &WorldDefinitionsView<'_>,
    objects: &Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: &Query<&GlobalTransform>,
    memberships: &InteractionContainerMemberships,
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
        commands.trigger(FinishObjectRelease { item, motion: None });
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
        .any(|(_, destination)| *destination == WorldObjectDetachDestination::Kill)
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
                    BehaviorWorldReleaseMotion::Fall { speed_mps: 0.0 }
                };
                let Some(world) = world else {
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
                commands.trigger(FinishObjectRelease {
                    item,
                    motion: Some(BehaviorWorldObjectReleased {
                        object: item,
                        world: world.root,
                        pose,
                        motion,
                    }),
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

pub(super) fn retire_owned_objects_when_holder_is_destroyed(
    despawn: On<Despawn<InteractionContainerSlots>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    memberships: InteractionContainerMemberships,
    mut carried: Query<(&ContainedObject, Option<&mut Transform>)>,
    anchors: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for item in memberships.members_of_container(despawn.entity) {
        let (Ok((contained, pose)), Some(slot)) = (
            carried.get_mut(item),
            memberships.actor_slot(despawn.entity, item),
        ) else {
            continue;
        };
        if super::holder::AttachmentHolder::find(&definitions, contained.holder_definition)
            .and_then(|holder| holder.place(slot))
            .is_some_and(|policy| policy.owns_contents)
        {
            commands.entity(item).try_despawn();
        } else if let Ok(anchor) = anchors.get(contained.joint.unwrap_or(despawn.entity)) {
            if let Some(mut pose) = pose {
                pose.set_if_neq(
                    anchor
                        .mul_transform(contained.relative_transform)
                        .compute_transform(),
                );
            }
        }
    }
}

pub(super) fn release_detached_object_presentation(
    removed: On<Remove<InteractionContainerMember>>,
    carried: Query<(), With<ContainedObject>>,
    mut commands: Commands,
) {
    if carried.contains(removed.entity) {
        commands.trigger(FinishObjectRelease {
            item: removed.entity,
            motion: None,
        });
    }
}

pub(super) fn finish_object_release(
    release: On<FinishObjectRelease>,
    mut items: Query<
        (
            Has<ContainedObject>,
            Option<&mut Transform>,
            Option<&mut Visibility>,
        ),
        Without<InteractionContainerMember>,
    >,
    mut commands: Commands,
) {
    let Ok((contained, transform, visibility)) = items.get_mut(release.item) else {
        return;
    };
    if !contained && release.motion.is_none() {
        return;
    }
    let mut item = commands.entity(release.item);
    if let Some(motion) = &release.motion {
        // An explicit release pose takes precedence over the old carry anchor.
        item.try_remove::<CarriedPrefabPresentation>();
        if let Some(mut transform) = transform {
            transform.set_if_neq(motion.pose);
        } else {
            item.try_insert(motion.pose);
        }
    }
    if let Some(mut visibility) = visibility {
        visibility.set_if_neq(Visibility::Inherited);
    } else {
        item.try_insert(Visibility::Inherited);
    }
    item.try_remove::<ContainedObject>();
    if let Some(motion) = &release.motion {
        commands.trigger(BehaviorWorldObjectReleased {
            object: motion.object,
            world: motion.world,
            pose: motion.pose,
            motion: motion.motion,
        });
    }
}

/// Text-key traversal may cross attach and detach in one simulation step. The
/// first key resolves the joint; this runs after deferred prefab admission, so
/// the later key cannot disappear merely because its item did not exist yet.
pub(super) fn apply_delivered_detach_after_object_admission(
    mut commands: Commands,
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
            &mut commands,
        ) {
            mark_behavior_task_for_failure_and_stop_navigation(holder, &mut commands);
        }
    }
}

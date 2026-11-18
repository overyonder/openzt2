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
    // Do not partially execute an unsupported authored outcome. Official
    // consumables create Trash/Recyclable directly in inventory.
    if rule
        .created_objects
        .iter()
        .any(|(_, destination)| !matches!(destination, WorldObjectDetachDestination::Container(_)))
        || (!rule.created_objects.is_empty() && world.is_none())
    {
        return false;
    }
    let created_transform = if rule.created_objects.is_empty() {
        None
    } else {
        let Ok(holder_transform) = transforms.get(holder) else {
            return false;
        };
        let Ok(joint_transform) = transforms.get(contained.joint.unwrap_or(holder)) else {
            return false;
        };
        Some((
            *joint_transform,
            joint_transform.reparented_to(holder_transform),
        ))
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
            // Drop/fall need the native placement/physics handoff, not a guessed
            // gravity switch or terrain snap.
            WorldObjectDetachDestination::Drop | WorldObjectDetachDestination::Fall => {
                return false;
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

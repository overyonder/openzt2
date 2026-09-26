use super::*;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;

pub(super) fn detach_slot_objects(
    holder: Entity,
    slot: usize,
    definitions: &WorldDefinitionsView<'_>,
    objects: &Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: &Query<&GlobalTransform>,
    ids: &mut PersistentIdAllocator,
    occupancy: &mut InteractionContainerOccupancy,
    commands: &mut Commands,
) -> bool {
    let members: Vec<_> = occupancy.members_in_slot(holder, slot).collect();
    for item in members {
        let Ok((contained, definition, world)) = objects.get(item) else {
            return false;
        };
        if contained.detach_rule == AssetId::default() {
            occupancy.remove_actor(item);
            commands
                .entity(item)
                .remove::<ContainedObject>()
                .insert(Visibility::Inherited);
            continue;
        }
        let Some(object) = definitions.find_object(definition.0) else {
            return false;
        };
        let Some(rule) = object
            .detach_actions
            .iter()
            .find(|rule| rule.name == contained.detach_rule)
        else {
            occupancy.remove_actor(item);
            commands.entity(item).despawn();
            continue;
        };
        // Do not partially execute an unsupported authored outcome. Official
        // consumables create Trash/Recyclable directly in inventory.
        if rule.created_objects.iter().any(|(_, destination)| {
            !matches!(destination, WorldObjectDetachDestination::Container(_))
        }) || (!rule.created_objects.is_empty() && world.is_none())
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
                    occupancy.remove_actor(item);
                    commands.entity(item).despawn();
                }
                WorldObjectDetachDestination::Container(destination) => {
                    let Some(holder_definition) =
                        definitions.find_object(contained.holder_definition)
                    else {
                        return false;
                    };
                    let Some((target_slot, target)) = holder_definition
                        .interaction_slots
                        .iter()
                        .enumerate()
                        .find(|(_, slot)| !slot.is_queue && slot.reservation_tag == destination)
                    else {
                        occupancy.remove_actor(item);
                        commands.entity(item).despawn();
                        return true;
                    };
                    if !occupancy.admit(holder, item, target_slot, target.capacity) {
                        occupancy.remove_actor(item);
                        commands.entity(item).despawn();
                        return true;
                    }
                    commands.entity(item).insert((
                        ContainedObject {
                            holder_definition: contained.holder_definition,
                            detach_rule: contained.detach_rule,
                            joint: None,
                            relative_transform: Transform::IDENTITY,
                            attachment_request: None,
                        },
                        if target.hides_contents {
                            Visibility::Hidden
                        } else {
                            Visibility::Inherited
                        },
                    ));
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
        if let (Some(world), Some((joint_transform, relative_transform))) =
            (world, created_transform)
        {
            created::reserve_detach_created_objects(
                holder,
                contained.holder_definition,
                *world,
                joint_transform,
                relative_transform,
                &rule.created_objects,
                definitions,
                ids,
                occupancy,
                commands,
            );
        }
    }
    true
}

pub(super) fn release_objects_from_destroyed_holders(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    objects: Query<(Entity, &ContainedObject)>,
    entities: Query<()>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (item, contained) in &objects {
        let Some((holder, slot)) = occupancy.container_for_member(item) else {
            commands
                .entity(item)
                .remove::<ContainedObject>()
                .insert(Visibility::Inherited);
            continue;
        };
        if entities.contains(holder) {
            continue;
        }
        let Some(policy) = definitions
            .find_object(contained.holder_definition)
            .and_then(|definition| definition.interaction_slots.get(slot))
        else {
            continue;
        };
        occupancy.remove_actor(item);
        if policy.owns_contents {
            commands.entity(item).despawn();
        } else {
            commands
                .entity(item)
                .remove::<ContainedObject>()
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
    mut ids: ResMut<PersistentIdAllocator>,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
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
        if let Some(slot) = occupancy.container_for_member(item) {
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
            &mut ids,
            &mut occupancy,
            &mut commands,
        ) {
            mark_behavior_task_for_failure_and_stop_navigation(holder, &mut commands);
        }
    }
}

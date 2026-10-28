//! Food props start at the source node, attach at the clip's text key and
//! detach before the authored continuation returns to the enclosing task.

use super::super::behavior_subject_type_resolution::{
    behavior_subject_type_identifiers, AnimalBehaviorSubjectTypes,
};
use super::*;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationCompleted, AnimationObjectCommand,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::{Dead, Frozen, Tranquilized};
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

#[cfg(test)]
mod tests;

#[derive(Component)]
struct HeldFoodPropAttachment {
    origin: PendingObjectAttachment,
    item: Option<Entity>,
    attached: bool,
    detached: bool,
    completed: bool,
}

pub(super) fn register(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_world_state::application_schedule::FixedGameSet;
    app.add_systems(FixedUpdate, (discard_cancelled_food_prop_attachments, execute_food_prop_attachments)
        .chain()
        .after(super::super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .before(super::start_or_finish_object_attachment)
        .in_set(FixedGameSet::Think)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, receive_food_prop_animation_events
            .in_set(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame)));
}

fn discard_cancelled_food_prop_attachments(
    mut commands: Commands,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    actors: Query<(
        Entity,
        &HeldFoodPropAttachment,
        Option<&BehaviorTaskExecutionState>,
        Has<PendingBehaviorTaskFailure>,
        Has<Dead>,
        Has<Frozen>,
        Has<Tranquilized>,
        Has<WorldEntityIsCrated>,
    )>,
    targets: Query<(), (With<WorldMember>, Without<WorldEntityIsCrated>)>,
    items: Query<(Entity, &FoodPropInstructionOwner)>,
) {
    for (actor, pending, task, failing, dead, frozen, tranquilized, crated) in &actors {
        if !failing
            && !dead
            && !frozen
            && !tranquilized
            && !crated
            && task.is_some_and(|task| pending.origin.owns(task))
            && pending
                .origin
                .target
                .is_some_and(|target| targets.contains(target))
        {
            continue;
        }
        if let Some(item) = pending.item.filter(|item| items.contains(*item)) {
            occupancy.remove_actor(item);
            commands.entity(item).despawn();
        }
        commands.entity(actor).remove::<HeldFoodPropAttachment>();
        if task.is_some_and(|task| pending.origin.owns(task)) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
    for (item, owner) in &items {
        if actors
            .get(owner.0)
            .is_ok_and(|(_, pending, ..)| pending.item == Some(item))
        {
            continue;
        }
        occupancy.remove_actor(item);
        commands.entity(item).despawn();
    }
}

/// The temporary prop belongs to this instruction until its detach rule runs.
#[derive(Component)]
struct FoodPropInstructionOwner(Entity);

fn execute_food_prop_attachments(
    mut commands: Commands,
    assets: AttachmentAssets,
    loaded: Res<LoadedBehaviorDocumentCollection>,
    animal_types: AnimalBehaviorSubjectTypes,
    clock: Res<ZooClock>,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &DefinitionId,
            &WorldMember,
            Option<&HeldFoodPropAttachment>,
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
        ),
    >,
    sources: Query<(&DefinitionId, &WorldMember), Without<WorldEntityIsCrated>>,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    nodes: Query<&PrefabAuthoredAttachmentIdentifier>,
) {
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (actor, mut task, actor_definition, world, pending, species, staff, guest) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::AttachObject(action)) = assets.documents.get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
            .filter(|action| matches!(action, BehaviorAction::AttachObject(action) if action.attaches_target_held_prop())) else { continue };
        if let Some(pending) =
            pending.filter(|pending| pending.origin.request_id.is_some() || pending.completed)
        {
            if !pending.detached && !pending.completed {
                continue;
            }
            if pending.item.is_some() && !pending.attached {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            let continuation = if let Some(set) = action.detach_behavior_set {
                let Some(view) = loaded.create_declaration_index_view(&assets.documents) else {
                    continue;
                };
                let Some(location) = view.find_behavior_set_location(
                    set,
                    behavior_subject_type_identifiers(
                        species,
                        staff,
                        guest,
                        definitions,
                        animal_types.borrow_for(actor),
                    ),
                ) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                Some((set, location))
            } else {
                None
            };
            if let Some(item) = pending.item {
                if !super::detach::detach_object(
                    actor,
                    item,
                    None,
                    &definitions,
                    &objects,
                    &transforms,
                    &mut ids,
                    &mut occupancy,
                    &mut commands,
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                commands
                    .entity(item)
                    .try_remove::<FoodPropInstructionOwner>();
            }
            commands.entity(actor).remove::<HeldFoodPropAttachment>();
            if let Some((set, (document, declaration))) = continuation {
                let frame = task.create_return_frame_after_action(task.action.saturating_add(1));
                if !task.push_return_frame(frame) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                task.program = set;
                task.document = document;
                task.declaration = declaration;
                task.phase = BehaviorTaskExecutionPhase::Set;
                task.action = 0;
                task.repetitions = 0;
                task.next_action_tick = clock.tick.saturating_add(1);
            } else {
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            }
            continue;
        }
        let Some(target) = task.target else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some(source) = sources
            .get(target)
            .ok()
            .filter(|(_, source_world)| source_world.root == world.root)
            .and_then(|(id, _)| definitions.find_object(id.0))
            .and_then(|source| source.container_quantity)
        else {
            if assets.definitions.is_complete() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        };
        let mut state = HeldFoodPropAttachment {
            origin: PendingObjectAttachment::waiting_for_attachment(&task, None),
            item: None,
            attached: source.held_prop.is_none(),
            detached: false,
            completed: action.animation.is_none(),
        };
        if let Some(name) = &action.subject_node {
            let identifier = AssetId::from_key(&name.to_ascii_lowercase());
            let Some(joint) = children
                .iter_descendants_depth_first::<Children>(actor)
                .find(|entity| nodes.get(*entity).is_ok_and(|node| node.0 == identifier))
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            state.origin.joint = Some(joint);
        }
        if let Some(prop) = source.held_prop {
            let Some(object) = definitions.find_object(prop) else {
                if assets.definitions.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            let Some((slot, place)) =
                holder::AttachmentHolder::find(&definitions, actor_definition.0).and_then(
                    |holder| {
                        holder
                            .place_for(action.container)
                            .and_then(|slot| Some((slot, holder.place(slot)?)))
                    },
                )
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let joint = if let Some(name) = &action.target_node {
                let identifier = AssetId::from_key(&name.to_ascii_lowercase());
                let Some(joint) = children
                    .iter_descendants_depth_first::<Children>(target)
                    .find(|entity| nodes.get(*entity).is_ok_and(|node| node.0 == identifier))
                else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                joint
            } else {
                target
            };
            let Ok(transform) = transforms.get(joint) else {
                continue;
            };
            let Some(handle) = pending
                .and_then(|pending| pending.origin.prefab.clone())
                .or_else(|| definitions.scene(object.prefab))
            else {
                if assets.definitions.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            state.origin.prefab = Some(handle.clone());
            let Some(prefab) = assets.prefabs.get(&handle) else {
                if matches!(
                    assets.server.get_load_state(handle.id()),
                    Some(LoadState::Failed(_))
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                } else {
                    commands.entity(actor).insert(state);
                }
                continue;
            };
            let Some(root) = prefab.canonical_scene_prefab_document().entities.first() else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            if occupancy.occupied_places(actor, slot) >= usize::from(place.capacity) {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            let Ok(id) = ids.allocate(world.root) else {
                continue;
            };
            let scale = Transform::from_scale(Vec3::splat(object.prefab_scale));
            let item = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                handle,
                world.root,
                object.id,
                id,
                transform.compute_transform().mul_transform(scale),
                true,
                None,
                RigidBody::Kinematic,
            );
            if !occupancy.admit(actor, item, slot, place.capacity) {
                commands.entity(item).despawn();
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            if let Some(modifier) = action.locomotion_modifier {
                commands.entity(actor).insert(
                    openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier {
                        modifier,
                        item,
                    },
                );
            }
            commands.entity(item).insert((
                ContainedObject {
                    holder_definition: actor_definition.0,
                    detach_rule: action.detach_rule,
                    joint: Some(joint),
                    relative_transform: scale
                        .mul_transform(transform_from_authored(&root.transform)),
                    attachment_request: None,
                },
                FoodPropInstructionOwner(actor),
                Visibility::Inherited,
            ));
            state.item = Some(item);
            state.attached = action.animation.is_none();
        }
        if let Some(animation) = &action.animation {
            state.origin.request_id = Some(
                requests
                    .write(AnimationClipPlaybackRequest {
                        animation_subject_entity: actor,
                        animation_clip_asset_key: animation.clone(),
                        blend_duration_milliseconds: 0,
                        playback_speed_permille: 1000,
                        playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                    })
                    .id,
            );
        }
        commands.entity(actor).insert(state);
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}

fn receive_food_prop_animation_events(
    mut commands: Commands,
    mut events: MessageReader<AnimationObjectCommand>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    mut actors: Query<
        (&BehaviorTaskExecutionState, &mut HeldFoodPropAttachment),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut props: Query<&mut ContainedObject>,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        let Ok((task, pending)) = actors.get(rejection.animation_subject_entity) else {
            continue;
        };
        if pending.origin.request_id == Some(rejection.request_id) && pending.origin.owns(task) {
            failed.insert(rejection.request_id);
            mark_behavior_task_for_failure_and_stop_navigation(
                rejection.animation_subject_entity,
                &mut commands,
            );
        }
    }
    for event in events.read() {
        let Ok((owner, playback)) = owners.get(event.controller) else {
            continue;
        };
        let Ok((task, mut pending)) = actors.get_mut(owner.gameplay_entity) else {
            continue;
        };
        if pending.origin.request_id.is_none()
            || event.playback_generation != playback.playback_generation
            || event.request_id != playback.explicit_clip_request_id
            || event.request_id != pending.origin.request_id
            || event.request_id.is_some_and(|id| failed.contains(&id))
            || !pending.origin.owns(task)
        {
            continue;
        }
        match &event.action {
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. } => {
                if let Some(item) = pending.item {
                    if let Ok(mut prop) = props.get_mut(item) {
                        prop.joint = Some(pending.origin.joint.unwrap_or(event.joint));
                        pending.attached = true;
                    }
                }
            }
            AuthoredAnimationTextAction::DetachObject if pending.attached => {
                pending.detached = true;
            }
            _ => {}
        }
    }
    for event in completed.read() {
        let Ok((owner, playback)) = owners.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let Ok((task, mut pending)) = actors.get_mut(owner.gameplay_entity) else {
            continue;
        };
        if pending.origin.request_id.is_some()
            && event.playback_generation == playback.playback_generation
            && event.explicit_clip_request_id == pending.origin.request_id
            && !event
                .explicit_clip_request_id
                .is_some_and(|id| failed.contains(&id))
            && pending.origin.owns(task)
        {
            pending.completed = true;
        }
    }
}

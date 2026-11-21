//! Authored carried objects use ordinary world entities and the shared container owner.

use super::behavior_animation_key_execution::CrossedBehaviorAnimationEffect;
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::{BehaviorAnimationEffect, TimedBehaviorAnimationEffect},
    PendingBehaviorAnimationClipCompletion,
};

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunBehaviorAttachObject, RunBehaviorDetachObject,
    RunningBehaviorInstruction,
};
mod admission;
mod created;
mod detach;
mod events;
mod held_clip;
mod held_food_prop;
mod holder;
mod presentation;
#[cfg(test)]
mod tests;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use super::behavior_container_admission::BehaviorAdmissionInstruction;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::{
    InteractionContainerMember, InteractionContainerMemberships,
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorHeldAnimationClip;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use avian3d::prelude::RigidBody;
use openzt2_zoo_state::animal_behavior::behavior_world_release_types::{
    BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion,
};
use bevy::{asset::LoadState, ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{AssetId};

#[derive(Component)]
struct PendingObjectAttachment {
    execution_id: u64,
    request_id: Option<usize>,
    prefab: Option<Handle<ScenePrefabAsset>>,
    detach_after_attachment: bool,
    instruction: BehaviorInstructionId,
    target: Option<Entity>,
    joint: Option<Entity>,
}

/// A valid text-key detach crossed before the corresponding prefab admission.
#[derive(Component)]
struct DeliveredAttachmentDetach;

fn release_detached_object_locomotion_modifiers(
    mut commands: Commands,
    memberships: InteractionContainerMemberships,
    actors: Query<(
        Entity,
        &openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier,
    )>,
    objects: Query<&ContainedObject>,
) {
    for (actor, modifier) in &actors {
        if objects.get(modifier.item).is_err()
            || memberships
                .container_for_member(modifier.item)
                .is_none_or(|(holder, _)| holder != actor)
        {
            commands.entity(actor).remove::<openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier>();
        }
    }
}

impl PendingObjectAttachment {
    fn waiting_for_attachment(
        task: &BehaviorTaskExecutionState,
        instruction: &RunningBehaviorInstruction,
        prefab: Option<Handle<ScenePrefabAsset>>,
    ) -> Self {
        Self {
            execution_id: task.execution_id,
            request_id: None,
            prefab,
            detach_after_attachment: false,
            instruction: instruction.id(),
            target: task.target,
            joint: None,
        }
    }

    fn owns(&self, task: &BehaviorTaskExecutionState, step: &RunningBehaviorInstruction) -> bool {
        self.instruction == step.id() && task.target == self.target && step.is_current(task)
    }
}

#[derive(SystemParam)]
pub(super) struct AnimationAttachmentKeyOwners<'w, 's> {
    attachments: Query<'w, 's, Option<&'static PendingObjectAttachment>>,
    food: Query<'w, 's, &'static held_food_prop::HeldFoodPropAttachment>,
}

impl AnimationAttachmentKeyOwners<'_, '_> {
    pub(super) fn owns_request(
        &self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        step: &RunningBehaviorInstruction,
        request: usize,
    ) -> bool {
        self.attachments
            .get(actor)
            .ok()
            .flatten()
            .or_else(|| self.food.get(actor).ok().map(|food| &food.origin))
            .is_some_and(|attachment| {
                attachment.request_id == Some(request) && attachment.owns(task, step)
            })
    }
}

#[derive(SystemParam)]
struct AttachmentAssets<'w> {
    documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    server: Res<'w, AssetServer>,
}

pub(super) fn register_object_attachment_execution(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_world_state::application_schedule::FixedGameSet;
    held_food_prop::register(app);
    events::register_attachment_animation_observers(app);
    app.add_observer(detach::retire_owned_objects_when_holder_is_destroyed)
        .add_observer(detach::release_detached_object_presentation)
        .add_observer(admission::apply_object_attachment_admissions)
        .add_observer(presentation::unbind_carried_prefab_presentation);
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        BehaviorStep, BehaviorStepSet,
    };
    app.add_systems(BehaviorStep, (
        start_or_finish_object_attachment,
        detach_held_objects_for_current_actions,
        detach::apply_delivered_detach_after_object_admission,
        created::hydrate_detach_created_objects,
        release_detached_object_locomotion_modifiers,
    ).chain().in_set(BehaviorStepSet::Actions)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, held_clip::release_behavior_held_animation_clips
            .in_set(FixedGameSet::Act)
            .after(events::receive_object_animation_commands)
            .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, events::receive_object_animation_commands
            .after(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorAnimationKeyDelivery)
            .in_set(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
            .in_set(FixedGameSet::Act)
            .before(super::behavior_animation_clip_execution::fail_behavior_animation_actions_after_playback_rejection)
            .before(super::behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_playback_completion)
            .run_if(in_state(GamePhase::InGame)))
        .add_systems(Update, presentation::bind_carried_prefab_presentation
            .before(openzt2_world_state::application_schedule::GameSet::Intent)
            .run_if(in_state(GamePhase::InGame)));
}

fn start_or_finish_object_attachment(
    mut commands: Commands,
    assets: AttachmentAssets,
    clock: Res<ZooClock>,
    memberships: InteractionContainerMemberships,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
    actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorAttachObject,
            ),
            &DefinitionId,
            &WorldMember,
            Option<&PendingObjectAttachment>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    obsolete: Query<
        Entity,
        (
            With<PendingObjectAttachment>,
            Or<(
                Without<BehaviorTaskExecutionState>,
                With<PendingBehaviorTaskFailure>,
            )>,
        ),
    >,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    attachment_nodes: Query<&PrefabAuthoredAttachmentIdentifier>,
    presentations: Query<&openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations>,
    animated: Query<
        (),
        With<openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController>,
    >,
) {
    for actor in &obsolete {
        commands.entity(actor).remove::<PendingObjectAttachment>();
    }
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (actor, (task, instruction, running_action), actor_definition, world, pending) in &actors {
        if pending.is_some_and(|pending| !pending.owns(task, instruction)) {
            commands.entity(actor).remove::<PendingObjectAttachment>();
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let action = &running_action.0;
        if action.attaches_target_held_prop() {
            continue;
        }
        let Some(holder) = holder::AttachmentHolder::find(&definitions, actor_definition.0) else {
            continue;
        };
        let Some((slot, place)) = holder
            .place_for(action.container)
            .and_then(|slot| Some((slot, holder.place(slot)?)))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        // `object` names the item the task uses, one the actor already holds,
        // such as the burger a guest takes from inventory to eat.
        let held_item = if action.entity == AssetId::from_key("object") {
            let Some(item) = task.object.filter(|item| {
                memberships
                    .container_for_member(*item)
                    .is_some_and(|(holder, _)| holder == actor)
            }) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            Some(item)
        } else {
            None
        };
        let Some(object) = held_item
            .map_or(Some(action.entity), |item| {
                objects.get(item).ok().map(|(_, id, _)| id.0)
            })
            .and_then(|id| definitions.find_object(id))
        else {
            if assets.definitions.is_complete() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        };
        let presentation = if held_item.is_some() || object.prefab == AssetId::default() {
            None
        } else {
            let Some(prefab_handle) = pending
                .and_then(|pending| pending.prefab.clone())
                .or_else(|| definitions.scene(object.prefab))
            else {
                if assets.definitions.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            let Some(prefab) = assets.prefabs.get(&prefab_handle) else {
                // Definitions retain source paths, not handles. Keep this demand
                // alive until the same pending action either commits or cancels.
                if pending.is_none() {
                    commands
                        .entity(actor)
                        .insert(PendingObjectAttachment::waiting_for_attachment(
                            task,
                            instruction,
                            Some(prefab_handle.clone()),
                        ));
                }
                if matches!(
                    assets.server.get_load_state(prefab_handle.id()),
                    Some(LoadState::Failed(_))
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            Some((prefab_handle, prefab))
        };
        let prefab_handle = presentation.as_ref().map(|(handle, _)| handle.clone());
        // A holder without animated presentation, such as a food stand's
        // static model, cannot play the authored clip: the item attaches at
        // once and its detach rule applies as the clip's end would.
        let plays_clip = action.animation.is_some()
            && openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations::for_subject(
                actor, presentations.get(actor).ok()
            ).any(|controller| animated.contains(controller));
        let detach_at_once = action.animation.is_some() && !plays_clip;
        if let Some(animation) = action.animation.as_ref().filter(|_| plays_clip) {
            if pending.is_none_or(|pending| pending.request_id.is_none()) {
                let request = requests.write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key: animation.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                });
                commands.entity(actor).insert((
                    PendingObjectAttachment {
                        request_id: Some(request.id),
                        ..PendingObjectAttachment::waiting_for_attachment(
                            task,
                            instruction,
                            prefab_handle.clone(),
                        )
                    },
                    BehaviorHeldAnimationClip {
                        request_id: request.id,
                    },
                ));
                continue;
            }
            if pending.is_none_or(|pending| pending.joint.is_none()) {
                continue;
            }
        }
        // A named subject node holds the item, as a guest's head holds a hat.
        let joint = match action.subject_node.as_deref() {
            Some(name) => {
                let identifier = AssetId::from_key(&name.to_ascii_lowercase());
                let Some(node) = children
                    .iter_descendants_depth_first::<Children>(actor)
                    .find(|node| {
                        attachment_nodes
                            .get(*node)
                            .is_ok_and(|attachment| attachment.0 == identifier)
                    })
                else {
                    warn!(?actor, node = name, "attachment subject node is absent");
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                node
            }
            None => pending.and_then(|pending| pending.joint).unwrap_or(actor),
        };
        let Ok(joint_transform) = transforms.get(joint) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let capacity = place.capacity;
        if held_item.is_none_or(|item| memberships.actor_slot(actor, item) != Some(slot))
            && memberships.occupied_places(actor, slot) >= usize::from(capacity)
            && !detach::detach_slot_objects(
                actor,
                slot,
                &definitions,
                &objects,
                &transforms,
                &memberships,
                &mut commands,
            )
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let scale = Transform::from_scale(Vec3::splat(object.prefab_scale));
        let relative_transform = if held_item.is_some() {
            definitions
                .scene(object.prefab)
                .and_then(|handle| assets.prefabs.get(&handle))
                .map_or(scale, |prefab| {
                    scale.mul_transform(transform_from_authored(
                        &prefab.canonical_scene_prefab_document().entities[0].transform,
                    ))
                })
        } else {
            presentation.as_ref().map_or(scale, |(_, prefab)| {
                scale.mul_transform(transform_from_authored(
                    &prefab.canonical_scene_prefab_document().entities[0].transform,
                ))
            })
        };
        let attachment = ContainedObject {
            holder_definition: actor_definition.0,
            detach_rule: action.detach_rule,
            joint: Some(joint),
            relative_transform,
            attachment_request: pending.and_then(|pending| {
                pending
                    .request_id
                    .map(|request| (request, pending.execution_id))
            }),
        };
        let modifier = action.locomotion_modifier;
        let visibility = if place.hides_contents {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        let detach =
            detach_at_once || pending.is_some_and(|pending| pending.detach_after_attachment);
        let tick = clock.tick;
        let spawn = held_item.is_none().then(|| admission::NewAttachedObject {
            world: *world,
            definition: object.id,
            instance_transform: joint_transform.compute_transform().mul_transform(scale),
            prefab: prefab_handle,
        });
        let admission = admission::AttachmentAdmission::Attach(admission::AttachAfterAdmission {
            actor,
            instruction: BehaviorAdmissionInstruction::new(actor, task, instruction),
            attachment,
            visibility,
            modifier,
            spawn,
            finish: admission::AttachmentAfterAdmission::Authored { tick, detach },
        });
        if let Some(item) = held_item {
            admission::request_existing_object_admission(
                &mut commands,
                item,
                actor,
                slot,
                capacity,
                admission,
            );
        } else {
            admission::request_new_object_admission(
                &mut commands,
                actor,
                slot,
                capacity,
                world.root,
                admission,
            );
        }
    }
}

/// Applies an authored detach rule to a held item, such as the trash left
/// once a guest has eaten.
fn detach_held_objects_for_current_actions(
    mut commands: Commands,
    assets: AttachmentAssets,
    clock: Res<ZooClock>,
    memberships: InteractionContainerMemberships,
    actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            Option<&RunBehaviorDetachObject>,
            Option<&TimedBehaviorAnimationEffect>,
            Option<&CrossedBehaviorAnimationEffect>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(
                With<RunBehaviorDetachObject>,
                With<TimedBehaviorAnimationEffect>,
            )>,
        ),
    >,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    transforms: Query<&GlobalTransform>,
) {
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (actor, task, instruction, immediate, timed, key) in &actors {
        if !instruction.is_current(task) {
            continue;
        }
        let (entity, rule) = match (immediate, timed) {
            (Some(immediate), _) if task.next_action_tick <= clock.tick => {
                (immediate.0.entity, immediate.0.detach_rule)
            }
            (
                _,
                Some(TimedBehaviorAnimationEffect(BehaviorAnimationEffect::Detach {
                    entity,
                    rule,
                })),
            ) if key.is_some_and(|key| key.belongs_to(instruction)) => (*entity, *rule),
            _ => continue,
        };
        let item = if entity == AssetId::from_key("object") {
            task.object
        } else {
            memberships.members_of_container(actor).find(|item| {
                objects.get(*item).is_ok_and(|(_, id, _)| {
                    entity == AssetId::default()
                        || id.0 == entity
                        || definitions.find_object(id.0).is_some_and(|object| {
                            object.behavior_type_identifiers.contains(&entity)
                        })
                })
            })
        };
        let Some(item) = item.filter(|item| {
            memberships
                .container_for_member(*item)
                .is_some_and(|(holder, _)| holder == actor)
        }) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if !detach::detach_object(
            actor,
            item,
            Some(rule),
            &definitions,
            &objects,
            &transforms,
            &mut commands,
        ) {
            warn!(
                ?actor,
                "authored detach rule has no supported runtime destination"
            );
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if timed.is_some() {
            commands.entity(actor).try_remove::<(
                CrossedBehaviorAnimationEffect,
                PendingBehaviorAnimationClipCompletion,
            )>();
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

#[derive(Component)]
pub(super) struct ContainedObject {
    holder_definition: AssetId,
    detach_rule: AssetId,
    joint: Option<Entity>,
    relative_transform: Transform,
    /// Request which attached this item; it may detach it later in the same clip.
    attachment_request: Option<(usize, u64)>,
}

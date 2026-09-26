//! Authored carried objects use ordinary world entities and the shared container owner.
mod created;
mod detach;
mod events;
mod presentation;
#[cfg(test)]
mod tests;

use super::{
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    interaction_container_occupancy::InteractionContainerOccupancy,
};
use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use crate::plugins::behavior_task_execution_types::advance_behavior_task_to_next_action;
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::behavior_task_execution_types::BehaviorTaskReturnFrame;
use crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::prefab_transform_conversion::transform_from_authored;
use crate::plugins::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::information::entity_selection_types::Inspectable;
use avian3d::prelude::RigidBody;
use bevy::{asset::LoadState, ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

#[derive(Component)]
pub(super) struct ContainedObject {
    holder_definition: AssetId,
    detach_rule: AssetId,
    joint: Option<Entity>,
    relative_transform: Transform,
    /// Request which attached this item; it may detach it later in the same clip.
    attachment_request: Option<(usize, u64)>,
}

#[derive(Component)]
struct PendingObjectAttachment {
    execution_id: u64,
    request_id: Option<usize>,
    prefab: Option<Handle<ScenePrefabAsset>>,
    detach_after_attachment: bool,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    target: Option<Entity>,
    joint: Option<Entity>,
}

/// A valid text-key detach crossed before the corresponding prefab admission.
#[derive(Component)]
struct DeliveredAttachmentDetach;

impl PendingObjectAttachment {
    fn waiting_for_attachment(
        task: &BehaviorTaskExecutionState,
        prefab: Option<Handle<ScenePrefabAsset>>,
    ) -> Self {
        Self {
            execution_id: task.execution_id,
            request_id: None,
            prefab,
            detach_after_attachment: false,
            origin: task.create_return_frame_after_action(task.action),
            depth: task.stack.len(),
            target: task.target,
            joint: None,
        }
    }

    fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.execution_id == self.execution_id
            && task.target == self.target
            && task.stack.len() == self.depth
            && task.is_at_or_nested_under(&self.origin, self.depth)
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
    use crate::{application_lifecycle::GamePhase, application_schedule::FixedGameSet};
    app.add_systems(FixedUpdate, (
        start_or_finish_object_attachment,
        detach::apply_delivered_detach_after_object_admission,
        created::hydrate_detach_created_objects,
        detach::release_objects_from_destroyed_holders,
    ).chain().in_set(FixedGameSet::Think)
        .before(super::interaction_container_occupancy::remove_destroyed_container_memberships)
        .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, events::receive_object_animation_commands
            .in_set(crate::plugins::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
            .in_set(FixedGameSet::Act)
            .before(super::behavior_animation_clip_execution::fail_behavior_animation_actions_after_playback_rejection)
            .before(super::behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_playback_completion)
            .run_if(in_state(GamePhase::InGame)))
        .add_systems(PostUpdate, presentation::project_contained_object_transforms
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate)
            .run_if(in_state(GamePhase::InGame)));
}

fn start_or_finish_object_attachment(
    mut commands: Commands,
    assets: AttachmentAssets,
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
) {
    for actor in &obsolete {
        commands.entity(actor).remove::<PendingObjectAttachment>();
    }
    let Some(definitions) = assets.definitions.get(&assets.definition_assets) else {
        return;
    };
    for (actor, mut task, actor_definition, world, pending) in &mut actors {
        if pending.is_some_and(|pending| !pending.owns(&task)) {
            commands.entity(actor).remove::<PendingObjectAttachment>();
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::AttachObject(action)) = assets
            .documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(holder) = definitions.find_object(actor_definition.0) else {
            continue;
        };
        let Some(slot) = holder.interaction_slots.iter().position(|slot| {
            !slot.is_queue && slot.reservation_tag == action.container && slot.capacity != 0
        }) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some(object) = definitions.find_object(action.entity) else {
            if assets.definitions.is_complete() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        };
        let presentation = if object.prefab == AssetId::default() {
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
                            &task,
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
        if let Some(animation) = &action.animation {
            if pending.is_none_or(|pending| pending.request_id.is_none()) {
                let request = requests.write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key: animation.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                });
                commands.entity(actor).insert(PendingObjectAttachment {
                    request_id: Some(request.id),
                    ..PendingObjectAttachment::waiting_for_attachment(&task, prefab_handle.clone())
                });
                continue;
            }
            if pending.is_none_or(|pending| pending.joint.is_none()) {
                continue;
            }
        }
        let joint = pending.and_then(|pending| pending.joint).unwrap_or(actor);
        let Ok(joint_transform) = transforms.get(joint) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let capacity = holder.interaction_slots[slot].capacity;
        if occupancy.occupied_places(actor, slot) >= usize::from(capacity)
            && !detach::detach_slot_objects(
                actor,
                slot,
                &definitions,
                &objects,
                &transforms,
                &mut ids,
                &mut occupancy,
                &mut commands,
            )
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if occupancy.occupied_places(actor, slot) >= usize::from(capacity) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let Ok(id) = ids.allocate(world.root) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let scale = Transform::from_scale(Vec3::splat(object.prefab_scale));
        let relative_transform = presentation.as_ref().map_or(scale, |(_, prefab)| {
            scale.mul_transform(transform_from_authored(
                &prefab.canonical_scene_prefab_document().entities[0].transform,
            ))
        });
        let instance_transform = joint_transform.compute_transform().mul_transform(scale);
        let item = if let Some((prefab_handle, prefab)) = presentation {
            spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                prefab_handle,
                world.root,
                object.id,
                id,
                instance_transform,
                true,
                None,
                RigidBody::Kinematic,
            )
        } else {
            commands
                .spawn((
                    *world,
                    DefinitionId(object.id),
                    id,
                    Inspectable {
                        definition: object.id,
                    },
                    instance_transform,
                    Visibility::Inherited,
                ))
                .id()
        };
        if !occupancy.admit(actor, item, slot, capacity) {
            commands.entity(item).despawn();
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        commands.entity(item).insert((
            ContainedObject {
                holder_definition: actor_definition.0,
                detach_rule: action.detach_rule,
                joint: Some(joint),
                relative_transform,
                attachment_request: pending.and_then(|pending| {
                    pending
                        .request_id
                        .map(|request| (request, pending.execution_id))
                }),
            },
            if holder.interaction_slots[slot].hides_contents {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        ));
        if pending.is_some_and(|pending| pending.detach_after_attachment) {
            commands.entity(item).insert(DeliveredAttachmentDetach);
        }
        commands.entity(actor).remove::<PendingObjectAttachment>();
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

use super::*;
use crate::plugins::{
    animation_graph::{
        animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected,
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_event_message_types::{AnimationCompleted, AnimationObjectCommand},
        animation_playback_controller_types::AnimationPlaybackController,
    },
    behavior_task_execution_types::PendingBehaviorAnimationClipCompletion,
};
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;

pub(super) fn receive_object_animation_commands(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AnimationObjectCommand>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut completed: MessageReader<AnimationCompleted>,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    tasks: Query<(
        Option<&BehaviorTaskExecutionState>,
        Option<&PendingBehaviorAnimationClipCompletion>,
        Has<PendingBehaviorTaskFailure>,
    )>,
    mut pending: Query<
        (&BehaviorTaskExecutionState, &mut PendingObjectAttachment),
        Without<PendingBehaviorTaskFailure>,
    >,
    actors: Query<&DefinitionId>,
    objects: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    transforms: Query<&GlobalTransform>,
    mut ids: ResMut<PersistentIdAllocator>,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        failed.insert(rejection.request_id);
        let Ok((task, state)) = pending.get(rejection.animation_subject_entity) else {
            continue;
        };
        if state.request_id == Some(rejection.request_id) && state.owns(task) {
            mark_behavior_task_for_failure_and_stop_navigation(
                rejection.animation_subject_entity,
                &mut commands,
            );
            commands
                .entity(rejection.animation_subject_entity)
                .remove::<PendingObjectAttachment>();
        }
    }
    for event in events.read() {
        let Ok((owner, playback)) = owners.get(event.controller) else {
            continue;
        };
        if event.playback_generation != playback.playback_generation
            || event.request_id != playback.explicit_clip_request_id
        {
            continue;
        }
        let actor = owner.gameplay_entity;
        match &event.action {
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. } => {
                let Ok((task, mut state)) = pending.get_mut(actor) else {
                    continue;
                };
                if state.request_id.is_some()
                    && event.request_id == state.request_id
                    && !state
                        .request_id
                        .is_some_and(|request| failed.contains(&request))
                    && state.owns(task)
                {
                    state.joint = Some(event.joint);
                }
            }
            AuthoredAnimationTextAction::DetachObject => {
                let Ok((task, animation, failing)) = tasks.get(actor) else {
                    continue;
                };
                if failing || event.request_id.is_some_and(|id| failed.contains(&id)) {
                    continue;
                }
                // Graph-owned playback has no task request. Explicit clips
                // must still belong to this execution, even after restart of
                // the same authored task at an identical action index.
                let owns_current_action = event.request_id.is_none()
                    || task.is_some_and(|task| {
                        animation.is_some_and(|animation| {
                            event.request_id == Some(animation.request_id)
                                && animation.still_owns_exact_task_frame(task)
                        }) || pending.get(actor).is_ok_and(|(_, pending)| {
                            pending.request_id.is_some()
                                && event.request_id == pending.request_id
                                && pending.owns(task)
                        })
                    });
                if let Ok((task, mut pending)) = pending.get_mut(actor) {
                    if pending.request_id.is_some()
                        && event.request_id == pending.request_id
                        && pending.owns(task)
                        && pending.joint.is_some()
                    {
                        pending.detach_after_attachment = true;
                    }
                }
                let Some(definitions) = definitions.get(&definition_assets) else {
                    continue;
                };
                let Some(definition) = actors
                    .get(actor)
                    .ok()
                    .and_then(|id| definitions.find_object(id.0))
                else {
                    continue;
                };
                for (slot, _) in
                    definition
                        .interaction_slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| {
                            !slot.is_queue && slot.reservation_tag == AssetId::default()
                        })
                {
                    // AttachObject advances at its attach key. Its still-live
                    // clip may legitimately detach that item at a later key.
                    let owns_attached_item = task.is_some_and(|task| {
                        occupancy.members_in_slot(actor, slot).any(|item| {
                            objects.get(item).is_ok_and(|(item, _, _)| {
                                item.attachment_request.is_some_and(|(request, execution)| {
                                    event.request_id == Some(request)
                                        && task.execution_id == execution
                                })
                            })
                        })
                    });
                    if !owns_current_action && !owns_attached_item {
                        continue;
                    }
                    if !detach::detach_slot_objects(
                        actor,
                        slot,
                        &definitions,
                        &objects,
                        &transforms,
                        &mut ids,
                        &mut occupancy,
                        &mut commands,
                    ) {
                        warn!(
                            ?actor,
                            "authored detach rule has no supported runtime destination"
                        );
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    }
                }
            }
            _ => {}
        }
    }
    for event in completed.read() {
        let Ok((owner, _)) = owners.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let Ok((task, state)) = pending.get(owner.gameplay_entity) else {
            continue;
        };
        if state.request_id.is_some()
            && event.explicit_clip_request_id == state.request_id
            && state.owns(task)
            && state.joint.is_none()
        {
            warn!(actor = ?owner.gameplay_entity, "attachment animation finished without its attach event");
            mark_behavior_task_for_failure_and_stop_navigation(
                owner.gameplay_entity,
                &mut commands,
            );
            commands
                .entity(owner.gameplay_entity)
                .remove::<PendingObjectAttachment>();
        }
    }
}

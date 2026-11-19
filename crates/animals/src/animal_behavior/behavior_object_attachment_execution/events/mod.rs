use super::*;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::RunningBehaviorInstruction;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;

#[derive(Event)]
struct ObjectAttachmentAnimationCommand(AnimationObjectCommand);

#[derive(Event)]
struct ObjectAttachmentAnimationCompleted(AnimationCompleted);

pub(super) fn register_attachment_animation_observers(app: &mut App) {
    app.add_observer(apply_object_animation_command)
        .add_observer(confirm_object_attachment_animation_completion);
}

pub(super) fn receive_object_animation_commands(
    mut commands: Commands,
    mut events: MessageReader<AnimationObjectCommand>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut completed: MessageReader<AnimationCompleted>,
    pending: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PendingObjectAttachment,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        failed.insert(rejection.request_id);
        let Ok((task, step, state)) = pending.get(rejection.animation_subject_entity) else {
            continue;
        };
        if state.request_id == Some(rejection.request_id) && state.owns(task, step) {
            debug!(actor = ?rejection.animation_subject_entity,
                clip = %rejection.animation_clip_asset_key,
                "object attachment clip was refused: no animated presentation has it");
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
        if event
            .request_id
            .is_none_or(|request| !failed.contains(&request))
        {
            // Apply each observer's commands before delivering the next key.
            commands.trigger(ObjectAttachmentAnimationCommand(event.clone()));
        }
    }
    for event in completed.read() {
        commands.trigger(ObjectAttachmentAnimationCompleted(event.clone()));
    }
}

#[derive(SystemParam)]
struct AttachmentAnimationQueries<'w, 's> {
    owners: Query<
        'w,
        's,
        (
            &'static AnimationPresentationOwner,
            &'static AnimationPlaybackController,
        ),
    >,
    tasks: Query<
        'w,
        's,
        (
            Option<&'static BehaviorTaskExecutionState>,
            Option<(
                &'static PendingBehaviorAnimationClipCompletion,
                &'static RunningBehaviorInstruction,
            )>,
            Has<PendingBehaviorTaskFailure>,
        ),
    >,
    pending: Query<
        'w,
        's,
        (
            &'static BehaviorTaskExecutionState,
            &'static RunningBehaviorInstruction,
            &'static mut PendingObjectAttachment,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    actors: Query<'w, 's, &'static DefinitionId>,
    objects: Query<
        'w,
        's,
        (
            &'static ContainedObject,
            &'static DefinitionId,
            Option<&'static WorldMember>,
        ),
    >,
    memberships: InteractionContainerMemberships<'w, 's>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
}

fn apply_object_animation_command(
    event: On<ObjectAttachmentAnimationCommand>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world_releases: Option<Res<BehaviorWorldReleaseSupport>>,
    queries: AttachmentAnimationQueries,
) {
    let AttachmentAnimationQueries {
        owners,
        tasks,
        mut pending,
        actors,
        objects,
        memberships,
        transforms,
    } = queries;
    let event = &event.0;
    let Ok((owner, playback)) = owners.get(event.controller) else {
        return;
    };
    if event.playback_generation != playback.playback_generation
        || event.request_id != playback.explicit_clip_request_id
    {
        return;
    }
    let actor = owner.gameplay_entity;
    match &event.action {
        AuthoredAnimationTextAction::AttachPendingObject
        | AuthoredAnimationTextAction::AttachNamedObject { .. } => {
            let Ok((task, step, mut state)) = pending.get_mut(actor) else {
                return;
            };
            if state.request_id.is_some()
                && event.request_id == state.request_id
                && state.owns(task, step)
            {
                state.joint = Some(event.joint);
            }
        }
        AuthoredAnimationTextAction::DetachObject => {
            let Ok((task, animation, failing)) = tasks.get(actor) else {
                return;
            };
            if failing {
                return;
            }
            // Graph-owned playback has no task request. Explicit clips
            // must still belong to this execution, even after restart of
            // the same authored task at an identical action index.
            let owns_current_action = event.request_id.is_none()
                || task.is_some_and(|task| {
                    animation.is_some_and(|(animation, instruction)| {
                        event.request_id == Some(animation.request_id)
                            && animation.owns(task, instruction)
                    }) || pending.get(actor).is_ok_and(|(_, step, pending)| {
                        pending.request_id.is_some()
                            && event.request_id == pending.request_id
                            && pending.owns(task, step)
                    })
                });
            if let Ok((task, step, mut pending)) = pending.get_mut(actor) {
                if pending.request_id.is_some()
                    && event.request_id == pending.request_id
                    && pending.owns(task, step)
                    && pending.joint.is_some()
                {
                    pending.detach_after_attachment = true;
                }
            }
            let Some(definitions) = definitions.get(&definition_assets) else {
                return;
            };
            let Some(holder) = actors
                .get(actor)
                .ok()
                .and_then(|id| super::holder::AttachmentHolder::find(&definitions, id.0))
            else {
                return;
            };
            for slot in holder.held_places() {
                // AttachObject advances at its attach key. Its still-live
                // clip may legitimately detach that item at a later key.
                let owns_attached_item = task.is_some_and(|task| {
                    memberships.members_in_slot(actor, slot).any(|item| {
                        objects.get(item).is_ok_and(|(item, _, _)| {
                            item.attachment_request.is_some_and(|(request, execution)| {
                                event.request_id == Some(request) && task.execution_id == execution
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
                    &memberships,
                    world_releases.as_deref(),
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

fn confirm_object_attachment_animation_completion(
    event: On<ObjectAttachmentAnimationCompleted>,
    mut commands: Commands,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    pending: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PendingObjectAttachment,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let event = &event.0;
    let Ok((owner, playback)) = owners.get(event.animation_playback_controller_entity) else {
        return;
    };
    let Ok((task, step, state)) = pending.get(owner.gameplay_entity) else {
        return;
    };
    if state.request_id.is_some()
        && event.explicit_clip_request_id == state.request_id
        && state.owns(task, step)
        && state.joint.is_none()
    {
        warn!(actor = ?owner.gameplay_entity, clip = %playback.animation_clip_asset_key,
                "attachment animation finished without its attach event");
        mark_behavior_task_for_failure_and_stop_navigation(owner.gameplay_entity, &mut commands);
        commands
            .entity(owner.gameplay_entity)
            .remove::<PendingObjectAttachment>();
    }
}

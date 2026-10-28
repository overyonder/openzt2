use bevy::prelude::*;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestStarted;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorHeldAnimationClip;

/// Returns the actor's presentation to locomotion once its held clip ends,
/// is refused, or is superseded by a later explicit request. Request
/// identifiers increase, so a deferred start of an older request is ignored.
pub(super) fn release_behavior_held_animation_clips(
    mut commands: Commands,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut started: MessageReader<AnimationClipPlaybackRequestStarted>,
    owners: Query<&AnimationPresentationOwner>,
    held: Query<&BehaviorHeldAnimationClip>,
) {
    let mut release = |actor: Entity, ended: bool| {
        if ended {
            commands
                .entity(actor)
                .try_remove::<BehaviorHeldAnimationClip>();
        }
    };
    for event in completed.read() {
        let Ok(owner) = owners.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let ended = held
            .get(actor)
            .is_ok_and(|clip| event.explicit_clip_request_id == Some(clip.request_id));
        release(actor, ended);
    }
    for event in rejected.read() {
        let actor = event.animation_subject_entity;
        let ended = held
            .get(actor)
            .is_ok_and(|clip| clip.request_id == event.request_id);
        release(actor, ended);
    }
    for event in started.read() {
        let actor = event.animation_subject_entity;
        let ended = held
            .get(actor)
            .is_ok_and(|clip| event.request_id > clip.request_id);
        release(actor, ended);
    }
}

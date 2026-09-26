//! Service readiness and timeout cancel the nested wait through the task owner.

use super::super::{
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    interaction_container_occupancy::InteractionContainerOccupancy,
};
use super::{ActiveQueueWait, QueueWaitSources};
use crate::plugins::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackState,
    },
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, BehaviorTaskExecutionState,
        PendingBehaviorAnimationClipCompletion, PendingBehaviorDockingCompletion,
        PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{Destination, Docking},
    simulation_time::simulation_clock_types::ZooClock,
};
use bevy::prelude::*;

pub(super) fn update_queue_wait_deadlines_and_service_readiness(
    mut commands: Commands,
    clock: Res<ZooClock>,
    sources: QueueWaitSources,
    occupancy: Res<InteractionContainerOccupancy>,
    mut actors: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        &ActiveQueueWait,
        Option<&PendingBehaviorTaskFailure>,
        Option<&PendingBehaviorAnimationClipCompletion>,
    )>,
    abandoned_waits: Query<Entity, (With<ActiveQueueWait>, Without<BehaviorTaskExecutionState>)>,
    mut animation_controllers: Query<(
        &AnimationPresentationOwner,
        &mut AnimationPlaybackController,
    )>,
) {
    for entity in &abandoned_waits {
        commands.entity(entity).remove::<ActiveQueueWait>();
    }
    let Some(definitions) = sources.definitions.get(&sources.definition_assets) else {
        return;
    };
    for (actor, mut task, wait, failure, pending_animation) in &mut actors {
        if failure.is_some() || !wait.still_owns_task(&task) {
            commands.entity(actor).remove::<ActiveQueueWait>();
            continue;
        }
        let object = sources
            .targets
            .get(wait.target)
            .ok()
            .and_then(|id| definitions.find_object(id.0));
        let ready = object.is_some_and(|object| {
            occupancy.queue_front_can_enter_service(
                wait.target,
                actor,
                wait.queue_slot,
                &object.interaction_slots,
            )
        });
        let lost_membership = occupancy.actor_slot(wait.target, actor) != Some(wait.queue_slot);
        if !ready && object.is_some() && !lost_membership && clock.tick < wait.deadline_tick {
            continue;
        }
        if let Some(pending) = pending_animation {
            // Cancel only the clip belonging to the interrupted wait. A newer
            // presentation request must not be stopped by an older task.
            let clip = sources.behaviors.get(&task.document)
                .and_then(|document| crate::plugins::behavior_task_execution_types::find_current_behavior_task_action(document, &task))
                .and_then(|action| pending.clip_key(action));
            if let Some(clip) = clip {
                for (owner, mut controller) in &mut animation_controllers {
                    if owner.gameplay_entity == actor
                        && controller.animation_clip_asset_key == clip
                        && controller.explicit_clip_request_id == Some(pending.request_id)
                        && pending.still_owns_exact_task_frame(&task)
                    {
                        controller.playback_state = AnimationPlaybackState::Paused;
                    }
                }
            }
        }
        wait.restore_wait_instruction(&mut task, clock.tick);
        commands.entity(actor).remove::<(
            ActiveQueueWait,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
            Destination,
            Docking,
        )>();
        if ready {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}

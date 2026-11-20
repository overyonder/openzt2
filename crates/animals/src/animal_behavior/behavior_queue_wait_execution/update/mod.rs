//! Service readiness and timeout cancel the nested wait through the task owner.

use super::super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use super::{ActiveQueueWait, QueueWaitSources};
use bevy::prelude::*;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
use openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    ResumeSuspendedBehaviorInstruction, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;

pub(super) fn update_queue_wait_deadlines_and_service_readiness(
    mut commands: Commands,
    clock: Res<ZooClock>,
    sources: QueueWaitSources,
    memberships: InteractionContainerMemberships,
    frames: BehaviorContinuationFrames,
    actors: Query<(
        Entity,
        &BehaviorTaskExecutionState,
        &ActiveQueueWait,
        Option<&PendingBehaviorTaskFailure>,
        Option<(
            &PendingBehaviorAnimationClipCompletion,
            &RunningBehaviorInstruction,
        )>,
    )>,
    abandoned_waits: Query<Entity, (With<ActiveQueueWait>, Without<BehaviorTaskExecutionState>)>,
    presentations: Query<&AnimationPresentations>,
    mut animation_controllers: Query<(&AnimationPlaybackController, &mut AnimationPlayer)>,
) {
    for entity in &abandoned_waits {
        commands.entity(entity).remove::<ActiveQueueWait>();
    }
    let Some(definitions) = sources.definitions.get(&sources.definition_assets) else {
        return;
    };
    for (actor, task, wait, failure, pending_animation) in &actors {
        if failure.is_some() || !wait.still_owns_task(actor, task, &frames) {
            commands.entity(actor).remove::<ActiveQueueWait>();
            continue;
        }
        let object = sources
            .targets
            .get(wait.target)
            .ok()
            .and_then(|id| definitions.find_object(id.0));
        let ready = object.is_some_and(|object| {
            memberships.queue_front_can_enter_service(
                wait.target,
                actor,
                wait.queue_slot,
                &object.interaction_slots,
            )
        });
        let lost_membership = memberships.actor_slot(wait.target, actor) != Some(wait.queue_slot);
        if !ready && object.is_some() && !lost_membership && clock.tick < wait.deadline_tick {
            continue;
        }
        if let Some((pending, _)) =
            pending_animation.filter(|(pending, instruction)| pending.owns(&task, instruction))
        {
            for controller in
                AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
            {
                let Ok((controller, mut player)) = animation_controllers.get_mut(controller) else {
                    continue;
                };
                if controller.animation_clip_asset_key == pending.clip_key
                    && controller.explicit_clip_request_id == Some(pending.request_id)
                {
                    player.pause_all();
                }
            }
        }
        commands.trigger(ResumeSuspendedBehaviorInstruction {
            actor,
            execution_id: wait.execution_id,
            origin: wait.origin.clone(),
            depth: wait.stack_depth,
            instruction: wait.origin.clone(),
            tick: clock.tick,
            advance: ready,
        });
        commands.entity(actor).remove::<(
            ActiveQueueWait,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion,
            Destination,
            Docking,
        )>();
        if !ready {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}

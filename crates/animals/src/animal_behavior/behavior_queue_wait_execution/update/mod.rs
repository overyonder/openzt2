//! Service readiness and timeout cancel the nested wait through the task owner.

use super::{ActiveQueueWait, QueueWaitSources};
use bevy::{
    ecs::{entity::EntityHashSet, relationship::Relationship},
    prelude::*,
};
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::{
    InteractionContainerMember, InteractionContainerMemberships, InteractionContainerSlotMembers,
    InteractionContainerSlotOf,
};
use openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorResumeOutcome, ResumeSuspendedBehaviorInstruction, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;

pub(super) fn update_queue_wait_deadlines_and_service_readiness<const DEADLINE_CHECK: bool>(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    sources: QueueWaitSources,
    memberships: InteractionContainerMemberships,
    frames: BehaviorContinuationFrames,
    changed_slots: Query<Entity, Changed<InteractionContainerSlotMembers>>,
    mut emptied_slots: RemovedComponents<InteractionContainerSlotMembers>,
    slots: Query<&InteractionContainerSlotOf>,
    mut removed_members: RemovedComponents<InteractionContainerMember>,
    mut removed_definitions: RemovedComponents<
        openzt2_world_state::world_spawn::world_membership_types::DefinitionId,
    >,
    actors: Query<(
        Entity,
        Ref<BehaviorTaskExecutionState>,
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
    if DEADLINE_CHECK {
        for entity in &abandoned_waits {
            commands.entity(entity).remove::<ActiveQueueWait>();
        }
    }
    let Some(definitions) = sources.definitions.get(&sources.definition_assets) else {
        return;
    };
    let mut changed_containers = EntityHashSet::default();
    let definitions_changed = !DEADLINE_CHECK
        && (sources.definitions.is_changed() || sources.definition_assets.is_changed());
    if !DEADLINE_CHECK {
        changed_containers.extend(
            slots
                .iter_many(changed_slots.iter().chain(emptied_slots.read()))
                .flatten()
                .map(Relationship::get),
        );
        changed_containers.extend(removed_definitions.read());
    }
    let mut affected = EntityHashSet::default();
    if !DEADLINE_CHECK && !definitions_changed {
        affected.extend(removed_members.read());
        for target in changed_containers {
            affected.extend(memberships.members_of_container(target));
        }
    }
    let mut full_pass = (DEADLINE_CHECK || definitions_changed).then(|| actors.iter());
    let mut affected = actors.iter_many(affected).flatten();
    while let Some((actor, task, wait, failure, pending_animation)) = match &mut full_pass {
        Some(actors) => actors.next(),
        None => affected.next(),
    } {
        if failure.is_some() || (task.is_changed() && !wait.still_owns_task(actor, &task, &frames))
        {
            commands.entity(actor).remove::<ActiveQueueWait>();
            continue;
        }
        if DEADLINE_CHECK && clock.tick < wait.deadline_tick {
            continue;
        }
        if !wait.still_owns_task(actor, &task, &frames) {
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
        commands.entity(actor).remove::<(
            ActiveQueueWait,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion,
            Destination,
            Docking,
        )>();
        commands.trigger(ResumeSuspendedBehaviorInstruction {
            actor,
            execution_id: wait.execution_id,
            origin: wait.origin.clone(),
            depth: wait.stack_depth,
            instruction: wait.origin.clone(),
            tick: clock.tick,
            outcome: if ready {
                BehaviorResumeOutcome::Advance
            } else {
                BehaviorResumeOutcome::Fail
            },
        });
    }
}

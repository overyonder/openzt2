use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
};

pub(super) struct BehaviorAdmissionInstruction {
    actor: Entity,
    execution_id: u64,
    target: Option<Entity>,
    due_tick: u64,
    frame: BehaviorTaskReturnFrame,
    depth: usize,
}

impl BehaviorAdmissionInstruction {
    pub(super) fn new(actor: Entity, task: &BehaviorTaskExecutionState) -> Self {
        Self {
            actor,
            execution_id: task.execution_id,
            target: task.target(),
            due_tick: task.next_action_tick,
            frame: task.create_return_frame_after_action(task.action),
            depth: task.stack.len(),
        }
    }

    pub(super) fn is_current(&self, world: &World) -> bool {
        world
            .get::<PendingBehaviorTaskFailure>(self.actor)
            .is_none()
            && world
                .get::<BehaviorTaskExecutionState>(self.actor)
                .is_some_and(|task| {
                    task.execution_id == self.execution_id
                        && task.target() == self.target
                        && task.next_action_tick == self.due_tick
                        && task.stack.len() == self.depth
                        && task.is_at_or_nested_under(&self.frame, self.depth)
                })
    }
}

#[cfg(test)]
pub(super) fn add_test_container_member(
    world: &mut World,
    container: Entity,
    member: Entity,
    slot: usize,
) {
    use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
        InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest,
    };
    let admission = InteractionContainerAdmissionRequest::Admit {
        container,
        member,
        slots: vec![(slot, u8::MAX)],
        despawn_on_refusal: false,
        queue_service_slots: vec![],
    };
    assert!(matches!(
        admission.commit(world),
        InteractionContainerAdmissionOutcome::Granted { .. }
    ));
}

#[cfg(test)]
pub(super) fn test_container_member_location(
    world: &mut World,
    member: Entity,
) -> Option<(Entity, usize)> {
    use bevy::ecs::system::RunSystemOnce;
    use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
    world
        .run_system_once(move |memberships: InteractionContainerMemberships| {
            memberships.container_for_member(member)
        })
        .unwrap()
}

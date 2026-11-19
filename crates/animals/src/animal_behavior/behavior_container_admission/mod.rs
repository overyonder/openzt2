use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::{BehaviorInstructionId, RunningBehaviorInstruction},
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};

pub(super) struct BehaviorAdmissionInstruction {
    actor: Entity,
    instruction: BehaviorInstructionId,
    target: Option<Entity>,
    due_tick: u64,
}

impl BehaviorAdmissionInstruction {
    pub(super) fn new(
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        instruction: &RunningBehaviorInstruction,
    ) -> Self {
        Self {
            actor,
            instruction: instruction.id(),
            target: task.target(),
            due_tick: task.next_action_tick,
        }
    }

    pub(super) fn is_current(&self, world: &World) -> bool {
        world
            .get::<PendingBehaviorTaskFailure>(self.actor)
            .is_none()
            && world
                .get::<BehaviorTaskExecutionState>(self.actor)
                .is_some_and(|task| {
                    task.target() == self.target
                        && task.next_action_tick == self.due_tick
                        && world
                            .get::<RunningBehaviorInstruction>(self.actor)
                            .is_some_and(|instruction| {
                                instruction.id() == self.instruction && instruction.is_current(task)
                            })
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

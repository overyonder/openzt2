use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorAnimalShowEvent, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::shows::show_event_types::AuthoredAnimalShowEvent;

#[allow(clippy::type_complexity)]
pub(super) fn register_current_authored_animal_show_events(
    activation: On<WhenBehaviorAction<RunBehaviorAnimalShowEvent>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut performers: Query<
        (
            Entity,
            &WorldMember,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorAnimalShowEvent,
            ),
        ),
        (
            With<Animal>,
            Without<PendingBehaviorTaskFailure>,
            Without<PendingBehaviorSet>,
        ),
    >,
) {
    for (performer, world, (task, instruction, running_action)) in
        std::iter::once(performers.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let policy = &running_action.0;
        commands.spawn((
            *world,
            AuthoredAnimalShowEvent {
                performer,
                execution_id: task.execution_id,
                policy: policy.clone(),
            },
        ));
        instruction.complete(performer, clock.tick, &mut commands);
    }
}

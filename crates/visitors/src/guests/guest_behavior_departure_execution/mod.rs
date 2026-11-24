use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{RunBehaviorTermination, RunningBehaviorInstruction, WhenBehaviorAction},
        BehaviorTaskExecutionState,
    },
    guests::guest_simulation_types::{
        DeparturePending, Guest, GuestDeparted, GuestEducation, GuestSatisfaction,
    },
};

pub(super) fn remove_guest_after_authored_departure(
    activation: On<WhenBehaviorAction<RunBehaviorTermination>>,
    guests: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
            &WorldMember,
            &GuestSatisfaction,
            &GuestEducation,
        ),
        (With<Guest>, Without<DeparturePending>),
    >,
    mut departed: MessageWriter<GuestDeparted>,
    mut commands: Commands,
) {
    let guest = activation.entity;
    let Ok((task, instruction, termination, world, satisfaction, education)) = guests.get(guest)
    else {
        return;
    };
    if !instruction.is_current(task) || !termination.0.kill_subject {
        return;
    }
    departed.write(GuestDeparted {
        root: world.root,
        guest,
        satisfaction_permille: satisfaction.0,
        education_permille: education.0,
    });
    commands.entity(guest).insert_if_new(DeparturePending);
}

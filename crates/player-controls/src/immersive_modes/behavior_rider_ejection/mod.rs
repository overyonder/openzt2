use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_interface_state::immersive_modes::{
    immersive_mode_message_types::{ExitImmersiveMode, ModeExitReason},
    immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode, ImmersiveSubjectSessions},
};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::{RunBehaviorKickOffRider, RunningBehaviorInstruction, WhenBehaviorAction},
    BehaviorTaskExecutionState,
};

pub(super) fn eject_player_from_authored_subject(
    activation: On<WhenBehaviorAction<RunBehaviorKickOffRider>>,
    actors: Query<(
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &WorldMember,
        Option<&ImmersiveSubjectSessions>,
    )>,
    modes: Query<&ActiveImmersiveMode>,
    clocks: Query<&ZooClock>,
    mut exit: MessageWriter<ExitImmersiveMode>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((task, instruction, world, sessions)) = actors.get(actor) else {
        return;
    };
    let Ok(clock) = clocks.get(world.root) else {
        return;
    };
    if !instruction.is_current(task) {
        return;
    }
    for controller in sessions.into_iter().flat_map(RelationshipTarget::iter) {
        if modes
            .get(controller)
            .is_ok_and(|mode| mode.mode == ImmersiveMode::FirstPerson)
        {
            exit.write(ExitImmersiveMode {
                controller,
                reason: ModeExitReason::Cancelled,
            });
        }
    }
    instruction.complete(actor, clock.tick, &mut commands);
}

use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_behavior::behavior_world_release_types::{
        BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion,
    },
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation, RunBehaviorFall,
            RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState,
    },
};

pub(super) fn start_authored_fall(
    activation: On<WhenBehaviorAction<RunBehaviorFall>>,
    falls: Query<(
        &RunBehaviorFall,
        &RunningBehaviorInstruction,
        &BehaviorTaskExecutionState,
    )>,
    actors: Query<(&WorldMember, &Transform)>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((action, instruction, task)) = falls.get(actor) else {
        return;
    };
    if !instruction.is_current(task) {
        return;
    }
    let Ok((member, transform)) = actors.get(actor) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    commands.trigger(BehaviorWorldObjectReleased {
        object: actor,
        world: member.root,
        pose: *transform,
        motion: BehaviorWorldReleaseMotion::Fall {
            speed_mps: action.0.speed_mps,
        },
    });
}

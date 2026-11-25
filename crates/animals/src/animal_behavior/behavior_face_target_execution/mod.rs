use bevy::prelude::*;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types::BehaviorPlacementTravelTargets;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorFaceTarget, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};

pub(super) fn register_face_target_execution(application: &mut App) {
    application.add_observer(face_current_behavior_targets);
}

/// Turns the subject to its target's position when it starts, plus the
/// authored offset, unless its heading is already within the authored error.
/// Headings change at once, as docking aligns them.
fn face_current_behavior_targets(
    activation: On<WhenBehaviorAction<RunBehaviorFaceTarget>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorFaceTarget,
            ),
            &mut Transform,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    targets: Query<&GlobalTransform>,
    travel_sites: BehaviorPlacementTravelTargets,
) {
    for (actor, (task, instruction, running_action), mut transform) in
        std::iter::once(actors.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let action = running_action.0;
        if let Some(toward) = travel_sites
            .for_actor(actor, task)
            .and_then(|target| targets.get(target).ok())
            .and_then(|target| {
                (target.translation() - transform.translation)
                    .xz()
                    .try_normalize()
            })
        {
            // Actor models face local +Z.
            let wanted = toward.x.atan2(toward.y) + action.rotation_offset_radians;
            let current = transform.rotation * Vec3::Z;
            let current = current.x.atan2(current.z);
            let error = (wanted - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if error.abs() > action.rotation_tolerance_radians {
                transform.rotation = Quat::from_rotation_y(wanted);
            }
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

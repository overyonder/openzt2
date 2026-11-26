use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{
        BehaviorSelfToken, BehaviorTrickDestination, BehaviorTrickPropOrigin,
    },
    behavior_task_execution_types::{
        running_actions::{
            RunBehaviorTeleportToLocation, RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState,
    },
    locomotion::locomotion_types::{Destination, Docking, MotionProgress, Route, Velocity},
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

pub(super) fn teleport_trick_target(
    activate: On<WhenBehaviorAction<RunBehaviorTeleportToLocation>>,
    actors: Query<(
        &RunBehaviorTeleportToLocation,
        &RunningBehaviorInstruction,
        &BehaviorTaskExecutionState,
        &WorldMember,
    )>,
    tokens: Query<(&BehaviorSelfToken, Option<&BehaviorTrickDestination>)>,
    origins: Query<&BehaviorTrickPropOrigin>,
    worlds: Query<&ZooClock>,
    mut objects: ParamSet<(
        Query<&GlobalTransform>,
        Query<(
            &WorldMember,
            &mut Transform,
            &mut GlobalTransform,
            Option<&ChildOf>,
            Option<&mut Route>,
            Option<&mut Velocity>,
            Option<&mut LinearVelocity>,
            Option<&mut AngularVelocity>,
            Option<&mut MotionProgress>,
        )>,
    )>,
    mut commands: Commands,
) {
    let actor = activate.entity;
    let Ok((action, step, task, member)) = actors.get(actor) else {
        return;
    };
    if !step.is_current(task) {
        return;
    }
    let Ok(clock) = worlds.get(member.root) else {
        return;
    };
    let Some((token, destination)) = task.object.and_then(|entity| tokens.get(entity).ok()) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let Some(target) = token.payload.or(task.target) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let offset = action
        .0
        .position
        .map(Vec3::from_array)
        .or_else(|| destination.map(|destination| destination.0));
    let position = if action.0.reset_prop {
        origins
            .get(target)
            .ok()
            .map(|origin| origin.0 + offset.unwrap_or_default())
    } else {
        offset
    };
    let Some(position) = position.filter(|position| position.is_finite()) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let (parent, mut pose) = {
        let mut targets = objects.p1();
        let Ok((world, _, global, parent, ..)) = targets.get_mut(target) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            return;
        };
        if world.root != member.root {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            return;
        }
        (parent.map(ChildOf::parent), global.compute_transform())
    };
    pose.translation = position;
    let world_pose = GlobalTransform::from(pose);
    let local_pose = parent
        .and_then(|parent| objects.p0().get(parent).ok().copied())
        .map_or(pose, |parent| world_pose.reparented_to(&parent));
    let mut targets = objects.p1();
    let Ok((_, mut local, mut global, _, route, velocity, linear, angular, progress)) =
        targets.get_mut(target)
    else {
        return;
    };
    local.set_if_neq(local_pose);
    global.set_if_neq(world_pose);
    if let Some(mut route) = route {
        route.clear();
    }
    if let Some(mut velocity) = velocity {
        velocity.0 = Vec3::ZERO;
    }
    if let Some(mut velocity) = linear {
        velocity.0 = Vec3::ZERO;
    }
    if let Some(mut velocity) = angular {
        velocity.0 = Vec3::ZERO;
    }
    if let Some(mut progress) = progress {
        progress.last_position = position;
        progress.stationary_ticks = 0;
    }
    commands.entity(target).remove::<(Destination, Docking)>();
    step.complete(actor, clock.tick, &mut commands);
}

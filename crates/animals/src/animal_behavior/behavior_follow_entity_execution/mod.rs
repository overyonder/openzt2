use bevy::prelude::*;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    behavior_task_execution_types::{
        running_actions::{
            activated_behavior_actor, behavior_action_actors, update_waiting_behavior_actors,
            BehaviorStepFixedSet, FollowingEntity, RunBehaviorFollowEntity,
            RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Destination, DockAt, Docking, NavAgent, NavigationFailed, NavigationRequestGeneration,
    },
};

use super::{
    behavior_named_nodes::NamedNodeTransforms,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};

pub(super) fn register(app: &mut App) {
    app.add_observer(activated_behavior_actor::<RunBehaviorFollowEntity>.pipe(follow_target_nodes))
        .add_observer(stop_following_when_instruction_ends)
        .add_systems(
            FixedUpdate,
            update_waiting_behavior_actors
                .pipe(follow_target_nodes)
                .in_set(BehaviorStepFixedSet::Think),
        )
        .add_systems(
            FixedUpdate,
            handle_follow_navigation_failures.in_set(FixedGameSet::Act),
        );
}

fn follow_target_nodes(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    fixed_time: Res<Time<Fixed>>,
    nodes: NamedNodeTransforms,
    transforms: Query<(&GlobalTransform, &WorldMember)>,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorFollowEntity,
            &NavAgent,
            &mut NavigationRequestGeneration,
            Option<&mut BehaviorRandomStream>,
            Option<&mut FollowingEntity>,
            Option<(&mut Destination, &mut Docking)>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut navigation: MessageWriter<DockAt>,
) {
    for (actor, task, step, action, agent, mut generation, random, state, navigation_state) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !step.can_update_after_activation(clock.tick) {
            continue;
        }
        let Some((target, target_pose, subject_pose)) = task.target.and_then(|target| {
            let (subject_pose, source_world) = transforms.get(actor).ok()?;
            let (target_pose, target_world) = transforms.get(target).ok()?;
            (target != actor && source_world.root == target_world.root).then_some((
                target,
                target_pose,
                subject_pose,
            ))
        }) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let action = &action.0;
        let target_node = match action.target_node.as_deref() {
            Some(name) => nodes.pose(target, name),
            None => Some(target_pose),
        };
        let subject_node = match action.subject_node.as_deref() {
            Some(name) => nodes.pose(actor, name),
            None => Some(subject_pose),
        };
        let (Some(target_node), Some(subject_node)) = (target_node, subject_node) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let subject_offset = subject_node.translation() - subject_pose.translation();
        let goal = target_node.translation() - subject_offset;
        let world_forward = target_node.rotation() * Vec3::Z;
        let existing =
            state.filter(|state| state.instruction == step.id() && state.target == target);
        let (until_tick, request) = if let Some(mut state) = existing {
            if clock.tick >= state.until_tick {
                step.complete(actor, clock.tick, &mut commands);
                continue;
            }
            if let Some((mut destination, mut docking)) =
                navigation_state.filter(|(destination, docking)| {
                    destination.request_id == state.request_id
                        && docking.request_id == state.request_id
                })
            {
                destination.set_if_neq(Destination {
                    world: goal,
                    ..*destination
                });
                docking.set_if_neq(Docking {
                    point: goal,
                    forward: world_forward,
                    ..*docking
                });
                continue;
            }
            let request = generation.advance();
            state.request_id = request;
            (state.until_tick, request)
        } else {
            let [minimum, maximum] = action.play_seconds;
            let random = if minimum == maximum {
                0.0
            } else if let Some(mut random) = random {
                random.0.unit_f32()
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let duration = (maximum - minimum).mul_add(random, minimum);
            let ticks = (f64::from(duration) / fixed_time.timestep().as_secs_f64()).ceil() as u64;
            let until_tick = clock.tick.saturating_add(ticks);
            if ticks == 0 {
                step.complete(actor, clock.tick, &mut commands);
                continue;
            }
            let request = generation.advance();
            commands.entity(actor).insert(FollowingEntity {
                instruction: step.id(),
                target,
                until_tick,
                request_id: request,
            });
            (until_tick, request)
        };
        if until_tick > clock.tick {
            let local = target_pose.affine().inverse();
            navigation.write(DockAt {
                entity: actor,
                target,
                request_id: request,
                local_point: local.transform_point3(goal),
                local_forward: local.transform_vector3(world_forward),
                radius_m: agent.radius_m.max(0.05),
            });
        }
    }
}

fn stop_following_when_instruction_ends(
    removed: On<Remove<RunBehaviorFollowEntity>>,
    states: Query<&FollowingEntity>,
    destinations: Query<&Destination>,
    mut commands: Commands,
) {
    let actor = removed.entity;
    let Ok(state) = states.get(actor) else {
        return;
    };
    let mut actor_commands = commands.entity(actor);
    if destinations
        .get(actor)
        .is_ok_and(|destination| destination.request_id == state.request_id)
    {
        actor_commands.try_remove::<(Destination, Docking)>();
    }
    actor_commands.try_remove::<FollowingEntity>();
}

fn handle_follow_navigation_failures(
    mut failures: MessageReader<NavigationFailed>,
    actors: Query<
        (&RunningBehaviorInstruction, &FollowingEntity),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut commands: Commands,
) {
    for failure in failures.read() {
        if actors.get(failure.entity).is_ok_and(|(step, state)| {
            state.instruction == step.id() && state.request_id == failure.request_id
        }) {
            mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
        }
    }
}

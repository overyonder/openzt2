use super::{
    behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use avian3d::prelude::{Collider, ColliderDisabled, Sensor, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_data::behavior::action::specialized::BehaviorEscapeObstacleAction;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::interaction_container_membership::{
        DockedAtInteractionContainer, InteractionContainerMember, InteractionContainerMemberships,
    },
    behavior_task_execution_types::{
        continuations::BehaviorContinuationFrames,
        running_actions::{
            activated_behavior_actor, behavior_action_actors, update_waiting_behavior_actors,
            BehaviorObstaclePriority, BehaviorStepFixedSet, BehaviorTaskReplaced,
            RunBehaviorEscapeObstacle, RunBehaviorExitInteractionContainer,
            RunningBehaviorInstruction, SelectedBehaviorSet,
        },
        BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{DirectLocomotion, NavAgent},
    terrain::terrain_chunk_types::TerrainChunk,
};

#[derive(Component)]
struct EscapeTimer(Timer);

#[derive(Component)]
struct ExitingContainer {
    execution: u64,
    frame: BehaviorTaskReturnFrame,
    depth: usize,
    colliders: Vec<Entity>,
}

pub(super) fn register(app: &mut App) {
    app.add_observer(activated_behavior_actor::<RunBehaviorEscapeObstacle>.pipe(escape_obstacles))
        .add_observer(
            activated_behavior_actor::<RunBehaviorExitInteractionContainer>.pipe(escape_obstacles),
        )
        .add_observer(clear_escape_movement_on_remove)
        .add_observer(restore_replaced_exit_colliders)
        .add_observer(restore_cancelled_exit_colliders)
        .add_observer(restore_failed_exit_colliders)
        .add_systems(
            FixedUpdate,
            update_waiting_behavior_actors
                .pipe(escape_obstacles)
                .in_set(BehaviorStepFixedSet::Think),
        );
}

#[allow(clippy::too_many_arguments)]
fn escape_obstacles(
    activation: In<Option<Entity>>,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &WorldMember,
            Option<(&NavAgent, &Transform)>,
            Option<&RunBehaviorEscapeObstacle>,
            Option<&RunBehaviorExitInteractionContainer>,
            Option<&ExitingContainer>,
            Option<&mut EscapeTimer>,
            Option<&mut DirectLocomotion>,
        ),
        (
            Or<(
                With<RunBehaviorEscapeObstacle>,
                With<RunBehaviorExitInteractionContainer>,
            )>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    worlds: Query<&ZooClock>,
    time: Res<Time<Fixed>>,
    frames: BehaviorContinuationFrames,
    members: InteractionContainerMemberships,
    types: AnimalBehaviorSubjectTypes,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    spatial: SpatialQuery,
    (children, parents, colliders, priorities, terrain, poses): (
        Query<&Children>,
        Query<&ChildOf>,
        Query<(), (With<Collider>, Without<ColliderDisabled>, Without<Sensor>)>,
        Query<&BehaviorObstaclePriority>,
        Query<(), With<TerrainChunk>>,
        Query<&GlobalTransform>,
    ),
    mut commands: Commands,
) {
    for (actor, task, step, world, motion, escape, exit, exiting, timer, direct) in
        behavior_action_actors(&mut actors, activation.0)
    {
        let Ok(clock) = worlds.get(world.root) else {
            continue;
        };
        if !step.is_current(task)
            || task.next_action_tick > clock.tick
            || (activation.0.is_none() && !step.can_update_after_activation(clock.tick))
        {
            continue;
        }
        let mut exiting = exiting.filter(|exit| {
            exit.execution == task.execution_id && frames.owns(actor, task, &exit.frame, exit.depth)
        });
        if let Some(exit) = exit.filter(|_| exiting.is_none()) {
            let Some((container, _)) = members.container_for_member(actor) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            if !members.is_docked(container, actor) {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            let set = if let Some(program) = exit.0 {
                let Some((document, declaration)) = documents
                    .create_declaration_index_view(&assets)
                    .and_then(|view| {
                        view.find_behavior_set_location(
                            program,
                            types.borrow_for(actor).iter().copied(),
                        )
                    })
                else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                Some(SelectedBehaviorSet {
                    program,
                    document,
                    declaration,
                    tick: clock.tick,
                    repeat: Some(task.repetitions),
                    target: Some(Some(container)),
                })
            } else {
                None
            };
            let suppressed = std::iter::once(actor)
                .chain(children.iter_descendants::<Children>(actor))
                .filter(|entity| colliders.contains(*entity))
                .collect::<Vec<_>>();
            for &collider in &suppressed {
                commands.entity(collider).insert(ColliderDisabled);
            }
            commands.entity(actor).insert(ExitingContainer {
                execution: task.execution_id,
                frame: task.create_return_frame_after_action(task.action),
                depth: task.depth,
                colliders: suppressed,
            });
            if let Some(selection) = set {
                step.enter_set(actor, selection, &mut commands);
            }
            continue;
        }
        if exit.is_none() {
            exiting = None;
        }
        let Some((agent, pose)) = motion else {
            end_escape(actor, exiting, timer.is_some(), &mut commands);
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let config = escape.map_or_else(BehaviorEscapeObstacleAction::default, |escape| escape.0);
        let radius = agent.radius_m.max(0.01);
        let origin = pose.translation + Vec3::Y * (radius + 0.01);
        let candidate_filter = SpatialQueryFilter::from_excluded_entities([actor]);
        let obstructs = |entity: Entity| {
            colliders.contains(entity)
                && !terrain.contains(entity)
                && !parents
                    .iter_ancestors(entity)
                    .any(|parent| parent == actor || terrain.contains(parent))
        };
        let priority = |entity| priorities.get(entity).map_or(10_000, |priority| priority.0);
        let mut obstructed = false;
        spatial.shape_intersections_callback(
            &Collider::sphere(radius),
            origin,
            Quat::IDENTITY,
            &candidate_filter,
            |entity| {
                obstructed = obstructs(entity) && priority(entity) >= 10_000;
                !obstructed
            },
        );
        if !obstructed {
            end_escape(actor, exiting, timer.is_some(), &mut commands);
            step.complete(actor, clock.tick, &mut commands);
            continue;
        }
        let mut new_timer = Timer::new(config.time, TimerMode::Once);
        let started = timer.is_some();
        let timer = timer
            .map(|timer| timer.into_inner())
            .map_or(&mut new_timer, |timer| &mut timer.0);
        if timer.tick(time.delta()).is_finished() {
            end_escape(actor, exiting, started, &mut commands);
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let mut away = Vec2::ZERO;
        let mut best = 10_000;
        let buffer = config.radius_metres.unwrap_or(radius);
        spatial.shape_intersections_callback(
            &Collider::sphere(radius + buffer),
            origin,
            Quat::IDENTITY,
            &candidate_filter,
            |entity| {
                if let Some(position) = poses.get(entity).ok().filter(|_| obstructs(entity)) {
                    let priority = priority(entity);
                    if priority < best {
                        return true;
                    }
                    if priority > best {
                        away = Vec2::ZERO;
                        best = priority;
                    }
                    away += (origin - position.translation()).xz().normalize_or_zero();
                }
                true
            },
        );
        let world_direction = away.normalize_or_zero();
        let world_direction = Vec3::new(world_direction.x, 0.0, world_direction.y);
        let local = pose.rotation.inverse() * world_direction;
        let axes = Vec2::new(local.x, -local.z);
        if let Some(mut direct) = direct {
            if direct.local_axes != axes {
                direct.local_axes = axes;
            }
        } else {
            commands
                .entity(actor)
                .insert(DirectLocomotion { local_axes: axes });
        }
        if !started {
            commands.entity(actor).insert(EscapeTimer(new_timer));
        }
    }
}

fn end_escape(
    actor: Entity,
    exiting: Option<&ExitingContainer>,
    started: bool,
    commands: &mut Commands,
) {
    if let Some(exiting) = exiting {
        for &collider in &exiting.colliders {
            commands.entity(collider).try_remove::<ColliderDisabled>();
        }
        commands.entity(actor).try_remove::<(
            ExitingContainer,
            InteractionContainerMember,
            DockedAtInteractionContainer,
        )>();
    }
    if started {
        commands
            .entity(actor)
            .try_remove::<(EscapeTimer, DirectLocomotion)>();
    }
}

fn clear_escape_movement_on_remove(removed: On<Remove<EscapeTimer>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<DirectLocomotion>();
}

fn restore_cancelled_exit_colliders(
    removed: On<Remove<BehaviorTaskExecutionState>>,
    exits: Query<&ExitingContainer>,
    timers: Query<(), With<EscapeTimer>>,
    mut commands: Commands,
) {
    end_escape(
        removed.entity,
        exits.get(removed.entity).ok(),
        timers.contains(removed.entity),
        &mut commands,
    );
}

fn restore_failed_exit_colliders(
    failed: On<Add<PendingBehaviorTaskFailure>>,
    exits: Query<&ExitingContainer>,
    timers: Query<(), With<EscapeTimer>>,
    mut commands: Commands,
) {
    end_escape(
        failed.entity,
        exits.get(failed.entity).ok(),
        timers.contains(failed.entity),
        &mut commands,
    );
}

fn restore_replaced_exit_colliders(
    replaced: On<BehaviorTaskReplaced>,
    exits: Query<&ExitingContainer>,
    timers: Query<(), With<EscapeTimer>>,
    mut commands: Commands,
) {
    end_escape(
        replaced.actor,
        exits.get(replaced.actor).ok(),
        timers.contains(replaced.actor),
        &mut commands,
    );
}

//! Traversal clips move the actor's sampled attachment along an authored path.

use avian3d::prelude::ColliderDisabled;
use bevy::prelude::*;
use openzt2_game_assets::{
    animation::{
        animation_clip_source_queries::AnimationClipSources,
        animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
    },
    application_lifecycle::GamePhase,
};
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::traversal_animation_path_types::TraversalAnimationPath;
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
        },
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
    },
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    behavior_task_execution_types::{
        running_actions::{
            BehaviorInstructionId, RunBehaviorTraversalAnimation, RunBehaviorTraversalDock,
            RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Arrived, DockAt, NavAgent, NavigationFailed, NavigationRequestSequence,
    },
};

use super::behavior_docking_execution::{
    find_named_descendant_global_transform, NamedNodeTransforms,
};

#[derive(Component)]
struct TraversalDock {
    instruction: BehaviorInstructionId,
    path: Entity,
    request_id: u64,
}

#[derive(Component)]
struct TraversalAnimation {
    instruction: BehaviorInstructionId,
    path: Entity,
    request_id: usize,
    attachment: Vec3,
    exit: bool,
    disabled_collider: bool,
}

pub(super) fn register_behavior_traversal_execution(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (start_traversal_docks, start_traversal_animation)
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    ).add_systems(
        FixedUpdate,
        (finish_traversal_docks, animate_traversal)
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn find_path(
    target: Entity,
    name: &str,
    children: &Query<&Children>,
    paths: &Query<(&Name, &TraversalAnimationPath, &GlobalTransform)>,
) -> Option<Entity> {
    children
        .iter_descendants_depth_first::<Children>(target)
        .find(|entity| {
            paths
                .get(*entity)
                .is_ok_and(|(path, _, _)| path.as_str().eq_ignore_ascii_case(name))
        })
}

fn traversal_attachment(
    actor: Entity,
    node: &str,
    transform: &GlobalTransform,
    children: &Query<&Children>,
    named: &NamedNodeTransforms,
) -> Option<Vec3> {
    let node = find_named_descendant_global_transform(actor, node, children, named)?;
    Some(
        transform
            .affine()
            .inverse()
            .transform_point3(node.translation()),
    )
}

fn start_traversal_docks(
    clock: Res<ZooClock>,
    actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTraversalDock,
            &GlobalTransform,
            &NavAgent,
        ),
        (Without<TraversalDock>, Without<PendingBehaviorTaskFailure>),
    >,
    children: Query<&Children>,
    named: NamedNodeTransforms,
    paths: Query<(&Name, &TraversalAnimationPath, &GlobalTransform)>,
    sources: Res<Assets<AnimationClipSourceAsset>>,
    clips: Res<Assets<AnimationClip>>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut requests: MessageWriter<DockAt>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    for (actor, task, step, action, transform, agent) in &actors {
        if task.next_action_tick > clock.tick || !step.is_current(task) {
            continue;
        }
        let path = task
            .target
            .and_then(|target| find_path(target, &action.target_path, &children, &paths));
        let attachment =
            traversal_attachment(actor, &action.subject_node, transform, &children, &named);
        let (Some(path), Some(attachment)) = (path, attachment) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let (_, geometry, _) = paths.get(path).expect("resolved traversal path");
        let Some(entry) = geometry.local_pose(0.0, &sources, &clips) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if let Some(clip) = &action.clip {
            playback.write(AnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                animation_clip_asset_key: clip.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy:
                    AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
            });
        }
        let request_id = sequence.next();
        requests.write(DockAt {
            entity: actor,
            request_id,
            target: path,
            local_point: entry.translation - entry.rotation * attachment,
            local_forward: entry.rotation * Vec3::Z,
            radius_m: agent.radius_m.max(0.05),
        });
        commands.entity(actor).insert(TraversalDock {
            instruction: step.id(),
            path,
            request_id,
        });
        step.resume_at(actor, clock.tick.saturating_add(1), &mut commands);
    }
}

fn finish_traversal_docks(
    clock: Res<ZooClock>,
    mut failures: MessageReader<NavigationFailed>,
    mut arrivals: MessageReader<Arrived>,
    actors: Query<(
        Entity,
        &TraversalDock,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Has<PendingBehaviorTaskFailure>,
    )>,
    mut commands: Commands,
) {
    let mut failed = bevy::ecs::entity::EntityHashSet::default();
    for result in failures.read() {
        let Ok((actor, pending, Some((task, step)), false)) = actors.get(result.entity) else {
            continue;
        };
        if pending.instruction == step.id()
            && step.is_current(task)
            && pending.request_id == result.request_id
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            commands.entity(actor).remove::<TraversalDock>();
            failed.insert(actor);
        }
    }
    for result in arrivals.read() {
        let Ok((actor, pending, Some((task, step)), false)) = actors.get(result.entity) else {
            continue;
        };
        if pending.instruction == step.id()
            && step.is_current(task)
            && pending.request_id == result.request_id
            && result.target == Some(pending.path)
            && !failed.contains(&actor)
        {
            step.complete(actor, clock.tick, &mut commands);
            commands.entity(actor).remove::<TraversalDock>();
        }
    }
    for (actor, pending, step, failing) in &actors {
        if failing
            || step.is_none_or(|(task, step)| {
                pending.instruction != step.id() || !step.is_current(task)
            })
        {
            commands.entity(actor).remove::<TraversalDock>();
        }
    }
}

fn start_traversal_animation(
    clock: Res<ZooClock>,
    actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTraversalAnimation,
            &GlobalTransform,
            Has<ColliderDisabled>,
        ),
        (
            Without<TraversalAnimation>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    children: Query<&Children>,
    named: NamedNodeTransforms,
    paths: Query<(&Name, &TraversalAnimationPath, &GlobalTransform)>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    for (actor, task, step, action, transform, collider_disabled) in &actors {
        if task.next_action_tick > clock.tick || !step.is_current(task) {
            continue;
        }
        let path = task
            .target
            .and_then(|target| find_path(target, &action.target_path, &children, &paths));
        let attachment =
            traversal_attachment(actor, &action.subject_node, transform, &children, &named);
        let (Some(path), Some(attachment)) = (path, attachment) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let (_, geometry, _) = paths.get(path).expect("resolved traversal path");
        let request_id = playback
            .write(AnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                animation_clip_asset_key: geometry.clip_key.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
            })
            .id;
        commands.entity(actor).insert(TraversalAnimation {
            instruction: step.id(),
            path,
            request_id,
            attachment,
            exit: action.exit,
            disabled_collider: !collider_disabled,
        });
        if !collider_disabled {
            commands.entity(actor).insert(ColliderDisabled);
        }
        step.resume_at(actor, clock.tick.saturating_add(1), &mut commands);
    }
}

fn animate_traversal(
    clock: Res<ZooClock>,
    mut actors: Query<(
        Entity,
        &TraversalAnimation,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Has<PendingBehaviorTaskFailure>,
        &mut Transform,
    )>,
    paths: Query<(&TraversalAnimationPath, &GlobalTransform)>,
    animations: Query<(
        Option<&AnimationPresentationOwner>,
        &AnimationPlaybackController,
        &AnimationPlayer,
    )>,
    presentations: Query<&AnimationPresentations>,
    clip_sources: AnimationClipSources,
    sources: Res<Assets<AnimationClipSourceAsset>>,
    clips: Res<Assets<AnimationClip>>,
    sets: Res<Assets<AnimationSetAsset>>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut commands: Commands,
) {
    let completed = completed
        .read()
        .filter_map(|result| {
            let (owner, _, _) = animations
                .get(result.animation_playback_controller_entity)
                .ok()?;
            result.explicit_clip_request_id.map(|id| {
                (
                    owner.map_or(result.animation_playback_controller_entity, |owner| {
                        owner.gameplay_entity
                    }),
                    id,
                )
            })
        })
        .collect::<Vec<_>>();
    let rejected = rejected
        .read()
        .map(|result| (result.animation_subject_entity, result.request_id))
        .collect::<Vec<_>>();
    for (actor, motion, step, failing, mut transform) in &mut actors {
        let current = step.filter(|(task, step)| {
            !failing && motion.instruction == step.id() && step.is_current(task)
        });
        let clip_completed = completed.contains(&(actor, motion.request_id));
        let clip_rejected = rejected.contains(&(actor, motion.request_id));
        let path = paths.get(motion.path).ok();
        if let (Some((_, step)), Some((path, world_transform)), false) =
            (current, path, clip_rejected)
        {
            let progress = if clip_completed {
                Some(1.0)
            } else {
                AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
                    .filter_map(|entity| animations.get(entity).ok())
                    .find(|(_, controller, _)| {
                        controller.explicit_clip_request_id == Some(motion.request_id)
                    })
                    .and_then(|(_, controller, player)| {
                        let set = sets.get(&controller.animation_set_asset)?;
                        let record = clip_sources.get(set, &controller.animation_clip_asset_key)?;
                        let (_, node) = record.graph?;
                        let active = player.animation(node)?;
                        let clip = clips.get(record.clip)?;
                        Some((active.seek_time() / clip.duration()).clamp(0.0, 1.0))
                    })
            };
            if let Some(progress) = progress {
                let t = if motion.exit {
                    1.0 - progress
                } else {
                    progress
                };
                let Some(sample) = path.local_pose(t, &sources, &clips) else {
                    continue;
                };
                let local_rotation = sample.rotation;
                let rotation = world_transform.compute_transform().rotation * local_rotation;
                let attachment = rotation * (transform.scale * motion.attachment);
                transform.rotation = rotation;
                transform.translation =
                    world_transform.transform_point(sample.translation) - attachment;
            }
            if !clip_completed {
                continue;
            }
            step.complete(actor, clock.tick, &mut commands);
        } else if current.is_some() {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
        commands.entity(actor).remove::<TraversalAnimation>();
        if motion.disabled_collider {
            commands.entity(actor).remove::<ColliderDisabled>();
        }
    }
}

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
use openzt2_world::animation_playback::traversal_animation_path_binding::ConfigureTraversalSegmentTiming;
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
        animation_event_message_types::{AnimationCompleted, AnimationObjectCommandDelivery},
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
    },
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation,
    behavior_task_execution_types::{
        running_actions::{
            BehaviorInstructionId, RunBehaviorTraversalAnimation, RunBehaviorTraversalDock,
            RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Arrived, DockAt, NavAgent, NavigationFailed, NavigationRequestGeneration,
    },
};

use super::behavior_named_nodes::NamedNodeTransforms;

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
    previous_progress: f32,
    clip_ended: bool,
    segment_timing: Option<TraversalSegmentTiming>,
}

struct TraversalSegmentTiming {
    generation: u64,
    start: f32,
    target: f32,
    delay_updates: u8,
    updates: u32,
    segment_count: u32,
}

impl TraversalSegmentTiming {
    fn advance(&mut self) -> f32 {
        if self.delay_updates > 0 {
            self.delay_updates -= 1;
        } else if self.updates < self.segment_count {
            self.updates += 1;
        }
        self.start + (self.target - self.start) * (self.updates as f32 / self.segment_count as f32)
    }
}

pub(super) fn register_behavior_traversal_execution(app: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorTraversalDock>.pipe(start_traversal_docks));
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorTraversalAnimation>.pipe(start_traversal_animation));

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    app.add_systems(
        FixedUpdate,
        (
            update_waiting_behavior_actors.pipe(start_traversal_docks),
            update_waiting_behavior_actors.pipe(start_traversal_animation),
        )
            .in_set(BehaviorStepFixedSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    )
    .add_systems(
        FixedUpdate,
        (
            finish_traversal_docks,
            animate_traversal.after(AnimationObjectCommandDelivery),
        )
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn traversal_attachment(
    actor: Entity,
    node: &str,
    transform: &GlobalTransform,
    named: &NamedNodeTransforms,
) -> Option<Vec3> {
    let node = named.pose(actor, node)?;
    Some(
        transform
            .affine()
            .inverse()
            .transform_point3(node.translation()),
    )
}

fn start_traversal_docks(
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTraversalDock,
            &GlobalTransform,
            &NavAgent,
        ),
        (Without<TraversalDock>, Without<PendingBehaviorTaskFailure>),
    >,
    named: NamedNodeTransforms,
    paths: Query<(&Name, &TraversalAnimationPath, &GlobalTransform)>,
    sources: Res<Assets<AnimationClipSourceAsset>>,
    clips: Res<Assets<AnimationClip>>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut requests: MessageWriter<DockAt>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    for (actor, mut task, step, action, transform, agent) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !step.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick || !step.is_current(&task) {
            continue;
        }
        let path = task
            .target
            .and_then(|target| named.path(target, &action.target_path));
        let attachment = traversal_attachment(actor, &action.subject_node, transform, &named);
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
                action_settings: None,
            });
        }
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request_id = generation.advance();
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
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}

fn finish_traversal_docks(
    clock: Single<&ZooClock>,
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
            commands.entity(actor).remove::<TraversalDock>();
            step.complete(actor, clock.tick, &mut commands);
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
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
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
    named: NamedNodeTransforms,
    paths: Query<(&Name, &TraversalAnimationPath, &GlobalTransform)>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    for (actor, mut task, step, action, transform, collider_disabled) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !step.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick || !step.is_current(&task) {
            continue;
        }
        let path = task
            .target
            .and_then(|target| named.path(target, &action.target_path));
        let attachment = traversal_attachment(actor, &action.subject_node, transform, &named);
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
                action_settings: None,
            })
            .id;
        commands.entity(actor).insert(TraversalAnimation {
            instruction: step.id(),
            path,
            request_id,
            attachment,
            exit: action.exit,
            disabled_collider: !collider_disabled,
            previous_progress: 0.0,
            clip_ended: false,
            segment_timing: None,
        });
        if !collider_disabled {
            commands.entity(actor).insert(ColliderDisabled);
        }
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}

fn animate_traversal(
    clock: Single<&ZooClock>,
    mut actors: Query<(
        Entity,
        &mut TraversalAnimation,
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
    mut timing_requests: MessageReader<ConfigureTraversalSegmentTiming>,
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
    for request in timing_requests.read() {
        if request.segment_count == 0 || !request.completion_fraction.is_finite() {
            continue;
        }
        let Ok((owner, controller, _)) = animations.get(request.controller) else {
            continue;
        };
        if controller.playback_generation != request.playback_generation {
            continue;
        }
        let actor = owner.map_or(request.controller, |owner| owner.gameplay_entity);
        let Ok((_, mut motion, ..)) = actors.get_mut(actor) else {
            continue;
        };
        if controller.explicit_clip_request_id != Some(motion.request_id) {
            continue;
        }
        motion.segment_timing = Some(TraversalSegmentTiming {
            generation: request.playback_generation,
            start: motion.previous_progress,
            target: request.completion_fraction.clamp(0.0, 1.0),
            delay_updates: 5,
            updates: 0,
            segment_count: request.segment_count,
        });
    }
    for (actor, mut motion, step, failing, mut transform) in &mut actors {
        let current = step.filter(|(task, step)| {
            !failing && motion.instruction == step.id() && step.is_current(task)
        });
        if completed.contains(&(actor, motion.request_id)) {
            motion.clip_ended = true;
        }
        let clip_rejected = rejected.contains(&(actor, motion.request_id));
        let path = paths.get(motion.path).ok();
        if let (Some((_, step)), Some((path, world_transform)), false) =
            (current, path, clip_rejected)
        {
            let active = AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
                .filter_map(|entity| animations.get(entity).ok())
                .find(|(_, controller, _)| {
                    controller.explicit_clip_request_id == Some(motion.request_id)
                });
            if let Some((_, controller, _)) = active {
                if motion
                    .segment_timing
                    .as_ref()
                    .is_some_and(|timing| timing.generation != controller.playback_generation)
                {
                    motion.segment_timing = None;
                }
            }
            let owns_timing = active.is_some() || motion.clip_ended;
            let progress =
                if let Some(timing) = motion.segment_timing.as_mut().filter(|_| owns_timing) {
                    Some(timing.advance())
                } else if motion.clip_ended {
                    Some(1.0)
                } else {
                    active.and_then(|(_, controller, player)| {
                        let set = sets.get(&controller.animation_set_asset)?;
                        let record = clip_sources.get(set, &controller.animation_clip_asset_key)?;
                        let (_, node) = record.graph?;
                        let active = player.animation(node)?;
                        let clip = clips.get(record.clip)?;
                        Some((active.seek_time() / clip.duration()).clamp(0.0, 1.0))
                    })
                };
            if let Some(progress) = progress {
                motion.previous_progress = progress;
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
            if !motion.clip_ended
                || motion
                    .segment_timing
                    .as_ref()
                    .is_some_and(|timing| timing.updates < timing.segment_count)
            {
                continue;
            }
            retire_traversal_motion(actor, &motion, &mut commands);
            step.complete(actor, clock.tick, &mut commands);
        } else {
            retire_traversal_motion(actor, &motion, &mut commands);
            if current.is_some() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
        }
    }
}
fn retire_traversal_motion(actor: Entity, motion: &TraversalAnimation, commands: &mut Commands) {
    commands.entity(actor).remove::<TraversalAnimation>();
    if motion.disabled_collider {
        commands.entity(actor).remove::<ColliderDisabled>();
    }
}

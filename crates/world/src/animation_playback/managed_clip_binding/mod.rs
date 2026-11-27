use crate::animation_graph::model_animation_asset_attachment::InitialAnimationSeek;
use bevy::{
    animation::{animated_field, transition::AnimationTransitions, AnimationTargetId},
    platform::collections::HashSet,
    prelude::*,
};
use openzt2_world_state::animation_playback::locomotion_root_motion::LocomotionRootMotionCycle;

use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::ActiveAnimationClipOptions,
    animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState,
    animation_playback::{
        animation_joint_target_types::AnimationRigJoints,
        animation_playback_controller_types::{
            AnimationPlaybackClock, AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
            RetainedAnimationClipSource,
        },
    },
};

use crate::animation_graph::animation_graph_blend_timer_advancement::FrozenAnimationPoseBlend;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::SplashAnimationSeek;
use std::time::Duration;

use super::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

#[derive(Component, Debug)]
pub(crate) struct ManagedAnimationClip {
    pub(crate) generation: u64,
    pub(crate) node: AnimationNodeIndex,
    pub(super) clip: Handle<AnimationClip>,
    graph: Handle<AnimationGraph>,
    pub(super) root_motion: Option<BoundLocomotionRootMotion>,
    pub(super) clip_entry: bool,
}

#[derive(Debug)]
pub(super) struct BoundLocomotionRootMotion {
    pub(super) joint: openzt2_game_data::AssetId,
    pub(super) entry_translation: Option<Vec3>,
    pub(super) entry_rotation: Option<Quat>,
    pub(super) full_cycle: LocomotionRootMotionCycle,
    pub(super) loop_cycle: LocomotionRootMotionCycle,
}

#[derive(Component)]
pub(crate) struct FixedAnimationPlayer;

#[derive(Component)]
pub(crate) struct RealTimeAnimationPlayer;

#[derive(Component)]
pub(crate) struct StockRenderAnimationAdvance;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ClipBindingInvalidation<'w, 's, Lane: Component> {
    changed: Query<
        'w,
        's,
        Entity,
        (
            With<Lane>,
            Or<(
                Changed<AnimationPlaybackController>,
                Added<Lane>,
                Added<AnimationPlayer>,
                Added<ConsumedLocomotionRootMotion>,
            )>,
        ),
    >,
    removed_motion: RemovedComponents<'w, 's, ConsumedLocomotionRootMotion>,
    changed_clips: MessageReader<'w, 's, AssetEvent<AnimationClip>>,
}

pub(super) fn assign_animation_player_clock(
    inserted: On<Insert<AnimationPlaybackController>>,
    controllers: Query<(
        &AnimationPlaybackController,
        Has<FixedAnimationPlayer>,
        Has<RealTimeAnimationPlayer>,
    )>,
    mut commands: Commands,
) {
    let Ok((controller, fixed, real_time)) = controllers.get(inserted.entity) else {
        return;
    };
    let mut entity = commands.entity(inserted.entity);
    match controller.playback_clock {
        AnimationPlaybackClock::Simulation if !fixed => {
            entity
                .remove::<RealTimeAnimationPlayer>()
                .insert(FixedAnimationPlayer);
        }
        AnimationPlaybackClock::RealTime if !real_time => {
            entity
                .remove::<FixedAnimationPlayer>()
                .insert(RealTimeAnimationPlayer);
        }
        _ => {}
    }
}

pub(crate) fn bind_selected_clips<Lane: Component>(
    mut commands: Commands,
    sources: AnimationClipSources,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: Res<Assets<AnimationClip>>,
    virtual_time: Res<Time<Virtual>>,
    source_assets: Res<Assets<AnimationClipSourceAsset>>,
    mut joints: Query<(
        Entity,
        &Transform,
        Option<&mut StandardBevyAnimationJointBlendPose>,
    )>,
    mut changed_sets: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut changed_sources: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    presentation_owners: Query<&AnimationPresentationOwner>,
    splash_seeks: Query<&SplashAnimationSeek>,
    graph_seeks: Query<
        &crate::animation_graph::animation_graph_route_selection::GraphTransitionOptions,
    >,

    mut bindings: ParamSet<(
        ClipBindingInvalidation<Lane>,
        Query<
            (
                Entity,
                Ref<AnimationPlaybackController>,
                &mut AnimationPlayer,
                Option<&mut ManagedAnimationClip>,
                Option<&mut AnimationGraphHandle>,
                Option<&mut RetainedAnimationClipSource>,
                Option<&mut AnimationGraphPlaybackState>,
                Option<&AnimationRigJoints>,
                Option<&ActiveAnimationClipOptions>,
                Has<StockRenderAnimationAdvance>,
                Has<ConsumedLocomotionRootMotion>,
                Option<&mut FrozenAnimationPoseBlend>,
                Option<&mut AnimationTransitions>,
                Option<&InitialAnimationSeek>,
            ),
            With<Lane>,
        >,
    )>,
) {
    let changed_sets = changed_sets
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let changed_sources = changed_sources
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut invalidation = bindings.p0();
    let changed_clips = invalidation
        .changed_clips
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut affected = invalidation
        .changed
        .iter()
        .chain(invalidation.removed_motion.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let mut controllers = bindings.p1();
    if !changed_sets.is_empty() || !changed_sources.is_empty() || !changed_clips.is_empty() {
        affected.extend(controllers.iter().filter_map(
            |(entity, controller, _, existing, _, retained, ..)| {
                let source_changed = if let Some(source) = retained {
                    changed_sources.contains(&source.0.id())
                } else {
                    !changed_sources.is_empty()
                        && sets
                            .get(&controller.animation_set_asset)
                            .and_then(|set| {
                                sources.retained_handle(set, &controller.animation_clip_asset_key)
                            })
                            .is_some_and(|handle| changed_sources.contains(&handle.id()))
                };
                (changed_sets.contains(&controller.animation_set_asset.id())
                    || source_changed
                    || existing.is_some_and(|binding| changed_clips.contains(&binding.clip.id())))
                .then_some(entity)
            },
        ));
    }
    let mut controllers = controllers.iter_many_mut(affected).matched();
    while let Some((
        entity,
        controller,
        mut player,
        existing,
        graph_handle,
        retained,
        state,
        rig,
        options,
        stock_advance,
        consumes_motion,
        frozen_blend,
        transitions,
        initial_seek,
    )) = controllers.fetch_next()
    {
        if options
            .is_some_and(|options| controller.explicit_clip_request_id != Some(options.request_id))
        {
            commands
                .entity(entity)
                .try_remove::<ActiveAnimationClipOptions>();
        }
        let Some(set) = sets.get(&controller.animation_set_asset) else {
            continue;
        };
        let Some(record) = sources.get(set, &controller.animation_clip_asset_key) else {
            continue;
        };
        let Some((graph, node)) = record.graph else {
            continue;
        };
        let source_handle = sources.retained_handle(set, &controller.animation_clip_asset_key);
        let changed = existing.as_deref().is_none_or(|current| {
            current.generation != controller.playback_generation
                || current.node != node
                || current.clip != *record.clip
                || current.graph != *graph
        }) || player.animation(node).is_none();
        let source_changed = changed_clips.contains(&record.clip.id())
            || source_handle
                .as_ref()
                .is_some_and(|handle| changed_sources.contains(&handle.id()));
        let active = if changed {
            let subject = presentation_owners
                .get(entity)
                .map_or(entity, |owner| owner.gameplay_entity);
            let splash_seek = splash_seeks
                .get(subject)
                .ok()
                .filter(|seek| controller.explicit_clip_request_id == Some(seek.request));
            if let Some(seek) = splash_seek {
                if seek.seconds.is_finite() {
                    commands.entity(subject).try_remove::<SplashAnimationSeek>();
                }
            }
            if initial_seek.is_some() {
                commands.entity(entity).try_remove::<InitialAnimationSeek>();
            }
            let graph_options = graph_seeks.get(entity).ok();
            if graph_options.is_some() {
                commands.entity(entity).try_remove::<crate::animation_graph::animation_graph_route_selection::GraphTransitionOptions>();
            }
            let graph_options = graph_options
                .filter(|options| options.generation == controller.playback_generation);
            let seek = if let Some(seek) = splash_seek.filter(|seek| seek.seconds.is_finite()) {
                seek.seconds
            } else if let Some(seek) = graph_options.and_then(|options| options.seek) {
                seek
            } else if let Some(initial) =
                initial_seek.filter(|_| controller.playback_generation == 0)
            {
                initial.0
            } else if controller.preserve_seek_on_restart {
                player
                    .playing_animations()
                    .next()
                    .map_or(0.0, |(_, animation)| animation.seek_time())
            } else {
                0.0
            };
            let duration = state
                .and_then(|mut state| state.requested_blend_duration_milliseconds.take())
                .map(|milliseconds| Duration::from_millis(u64::from(milliseconds)));
            let duration = graph_options
                .and_then(|options| options.duration)
                .or(duration)
                .unwrap_or_default();
            let source = source_handle
                .as_ref()
                .and_then(|handle| source_assets.get(handle));
            let outgoing_has_events = retained
                .as_ref()
                .and_then(|handle| source_assets.get(&handle.0))
                .is_some_and(|source| !source.timeline.is_empty());
            let live_transition = controller.playback_clock == AnimationPlaybackClock::RealTime
                && !outgoing_has_events
                && !consumes_motion
                && !duration.is_zero()
                && existing.as_ref().is_some_and(|clip| {
                    clip.node != node
                        && clip.graph == *graph
                        && player
                            .animation(clip.node)
                            .is_some_and(|animation| !animation.is_paused())
                })
                && controller.playback_speed_permille > 0
                && !controller.preserve_seek_on_restart
                && source.is_none_or(|source| source.loop_bounds.is_none());
            let frozen_duration = if live_transition || existing.is_none() {
                Duration::ZERO
            } else {
                duration
            };
            match (frozen_blend, frozen_duration.is_zero()) {
                (Some(mut blend), false) => {
                    blend.0 = Timer::new(frozen_duration, TimerMode::Once);
                }
                (None, false) => {
                    commands
                        .entity(entity)
                        .try_insert(FrozenAnimationPoseBlend(Timer::new(
                            frozen_duration,
                            TimerMode::Once,
                        )));
                }
                (Some(_), true) => {
                    commands
                        .entity(entity)
                        .try_remove::<FrozenAnimationPoseBlend>();
                }
                (None, true) => {}
            }
            if !frozen_duration.is_zero() {
                if let Some(rig) = rig {
                    let mut joints = joints.iter_many_mut(rig.joints()).matched();
                    while let Some((joint, transform, snapshot)) = joints.fetch_next() {
                        let next = StandardBevyAnimationJointBlendPose {
                            playback_generation: controller.playback_generation,
                            pose: *transform,
                        };
                        if let Some(mut snapshot) = snapshot {
                            snapshot.set_if_neq(next);
                        } else {
                            commands.entity(joint).try_insert(next);
                        }
                    }
                }
            }
            let active = if live_transition {
                if let Some(mut transitions) = transitions {
                    transitions.play(&mut player, node, duration)
                } else {
                    let mut transitions = AnimationTransitions::new();
                    let outgoing = existing
                        .as_ref()
                        .expect("live transition requires outgoing clip")
                        .node;
                    let previous = *player
                        .animation(outgoing)
                        .expect("live transition requires a playing clip");
                    *transitions.play(&mut player, outgoing, Duration::ZERO) = previous;
                    let incoming = transitions.play(&mut player, node, duration);
                    commands.entity(entity).try_insert(transitions);
                    incoming
                }
            } else {
                if transitions.is_some() {
                    commands.entity(entity).try_remove::<AnimationTransitions>();
                }
                player.stop_all();
                player.start(node)
            };
            let seek = clips
                .get(record.clip)
                .map_or(seek, |clip| seek.clamp(0.0, clip.duration()));
            active.set_seek_time(seek);
            active
        } else {
            let Some(active) = player.animation_mut(node) else {
                continue;
            };
            active
        };
        active.set_speed(
            f32::from(controller.playback_speed_permille) / 1000.0
                * record.authored_playback_rate(),
        );
        let repeat = match controller.playback_repetition_policy {
            AnimationPlaybackRepetitionPolicy::Loop => true,
            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy => {
                record.authored_playback_is_looped()
            }
            AnimationPlaybackRepetitionPolicy::PlayOnce => false,
        };
        active.set_repeat(if repeat {
            bevy::animation::RepeatAnimation::Forever
        } else {
            bevy::animation::RepeatAnimation::Never
        });

        let source = source_handle
            .as_ref()
            .and_then(|handle| source_assets.get(handle));
        let clip = clips.get(record.clip);
        let use_stock_advance =
            controller.playback_clock == AnimationPlaybackClock::RealTime
                && active.speed() > 0.0
                && !controller.preserve_seek_on_restart
                && initial_seek.is_none()
                && !consumes_motion
                && clip.is_some_and(|clip| {
                    let duration = clip.duration();
                    duration > 0.0
                        && (!repeat
                            || (source.is_none_or(|source| {
                                source.loop_bounds.as_ref().is_none_or(|bounds| {
                                    bounds.start == 0.0 && bounds.end == duration
                                }) && source
                                    .timeline
                                    .first()
                                    .is_none_or(|key| key.milliseconds != 0)
                            }) && virtual_time.max_delta().as_secs_f32() * active.speed()
                                < duration))
                });
        match (use_stock_advance, stock_advance) {
            (true, false) => {
                commands
                    .entity(entity)
                    .try_insert(StockRenderAnimationAdvance);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .try_remove::<StockRenderAnimationAdvance>();
            }
            _ => {}
        }

        if changed || source_changed {
            let wanted = ManagedAnimationClip {
                generation: controller.playback_generation,
                node,
                clip: record.clip.clone(),
                graph: graph.clone(),
                clip_entry: !use_stock_advance
                    && (changed || existing.as_ref().is_some_and(|clip| clip.clip_entry)),
                root_motion: source_handle
                    .as_ref()
                    .and_then(|handle| source_assets.get(handle))
                    .zip(clips.get(record.clip))
                    .and_then(|(source, clip)| bind_clip_root_motion(source, clip)),
            };
            if let Some(mut existing) = existing {
                *existing = wanted;
            } else {
                commands.entity(entity).try_insert(wanted);
            }
            if let Some(mut graph_handle) = graph_handle {
                if graph_handle.0 != *graph {
                    graph_handle.0 = graph.clone();
                }
            } else {
                commands
                    .entity(entity)
                    .try_insert(AnimationGraphHandle(graph.clone()));
            }
        }
        if let Some(source) = source_handle {
            if let Some(mut retained) = retained {
                if retained.0 != source {
                    retained.0 = source;
                }
            } else {
                commands
                    .entity(entity)
                    .try_insert(RetainedAnimationClipSource(source));
            }
        } else if retained.is_some() {
            commands
                .entity(entity)
                .try_remove::<RetainedAnimationClipSource>();
        }
    }
}

fn bind_clip_root_motion(
    source: &AnimationClipSourceAsset,
    clip: &AnimationClip,
) -> Option<BoundLocomotionRootMotion> {
    let root_joint = source
        .authored_animation_clip_metadata
        .root_motion_joint
        .as_ref()?;
    let root = &root_joint.joint_asset_key;
    let joint = root_joint.joint_id();
    let target = AnimationTargetId::from_name(&Name::new(root.to_ascii_lowercase()));
    let range = 0.0..clip.duration();
    let full_cycle = LocomotionRootMotionCycle::from_clip(clip, root, range);
    Some(BoundLocomotionRootMotion {
        joint,
        entry_translation: clip.sample_clamped(
            animated_field!(Transform::translation),
            target,
            0.0,
        ),
        entry_rotation: clip.sample_clamped(animated_field!(Transform::rotation), target, 0.0),
        full_cycle,
        loop_cycle: source.loop_bounds.clone().map_or(full_cycle, |range| {
            LocomotionRootMotionCycle::from_clip(clip, root, range)
        }),
    })
}

pub(super) fn retire_unbound_players(
    mut commands: Commands,
    mut removed: RemovedComponents<AnimationPlaybackController>,
    retired: Query<
        Has<ManagedAnimationClip>,
        (
            Or<(With<FixedAnimationPlayer>, With<RealTimeAnimationPlayer>)>,
            Without<AnimationPlaybackController>,
        ),
    >,
) {
    for entity in removed.read() {
        let Ok(managed) = retired.get(entity) else {
            continue;
        };
        let mut retired = commands.entity(entity);
        retired.try_remove::<(
            FixedAnimationPlayer,
            RealTimeAnimationPlayer,
            StockRenderAnimationAdvance,
            super::locomotion_root_motion::BoundRootMotionJoints,
            FrozenAnimationPoseBlend,
            InitialAnimationSeek,
            AnimationTransitions,
        )>();
        if managed {
            retired.try_remove::<(
                ManagedAnimationClip,
                AnimationPlayer,
                AnimationGraphHandle,
                RetainedAnimationClipSource,
            )>();
        }
    }
}

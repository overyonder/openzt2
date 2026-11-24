use bevy::{
    animation::{animated_field, AnimationTargetId},
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

use super::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;

#[derive(Component, Debug)]
pub(super) struct ManagedAnimationClip {
    pub(super) generation: u64,
    pub(super) node: AnimationNodeIndex,
    pub(super) clip: Handle<AnimationClip>,
    graph: Handle<AnimationGraph>,
    pub(super) root_motion: Option<BoundLocomotionRootMotion>,
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
pub(super) struct FixedAnimationPlayer;

#[derive(Component)]
pub(super) struct RealTimeAnimationPlayer;

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

pub(super) fn bind_selected_clips<Lane: Component>(
    mut commands: Commands,
    sources: AnimationClipSources,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: Res<Assets<AnimationClip>>,
    source_assets: Res<Assets<AnimationClipSourceAsset>>,
    mut joints: Query<(
        Entity,
        &Transform,
        Option<&mut StandardBevyAnimationJointBlendPose>,
    )>,
    mut changed_sets: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut changed_sources: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    mut controllers: Query<
        (
            Entity,
            Ref<AnimationPlaybackController>,
            &mut AnimationPlayer,
            Option<&mut ManagedAnimationClip>,
            Option<&mut AnimationGraphHandle>,
            Option<&mut RetainedAnimationClipSource>,
            Option<&AnimationGraphPlaybackState>,
            Option<&AnimationRigJoints>,
            Option<&ActiveAnimationClipOptions>,
        ),
        With<Lane>,
    >,
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
    for (entity, controller, mut player, existing, graph_handle, retained, state, rig, options) in
        &mut controllers
    {
        if options.is_some_and(|options| {
            controller.explicit_clip_request_id != Some(options.0.request_id)
        }) {
            commands
                .entity(entity)
                .try_remove::<ActiveAnimationClipOptions>();
        }
        if !controller.is_changed()
            && existing.is_some()
            && !changed_sets.contains(&controller.animation_set_asset.id())
            && retained
                .as_ref()
                .is_none_or(|source| !changed_sources.contains(&source.0.id()))
        {
            continue;
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
        let source_changed = source_handle
            .as_ref()
            .is_some_and(|handle| changed_sources.contains(&handle.id()));
        let active = if changed {
            let seek = if controller.preserve_seek_on_restart {
                player
                    .playing_animations()
                    .next()
                    .map_or(0.0, |(_, animation)| animation.seek_time())
            } else {
                0.0
            };
            if existing.is_some()
                && state
                    .and_then(AnimationGraphPlaybackState::active_blend_duration_milliseconds)
                    .is_some()
            {
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
            player.stop_all();
            let active = player.start(node);
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

        if changed || source_changed {
            let wanted = ManagedAnimationClip {
                generation: controller.playback_generation,
                node,
                clip: record.clip.clone(),
                graph: graph.clone(),
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
    let root = &source
        .authored_animation_clip_metadata
        .root_motion_joint
        .as_ref()?
        .joint_asset_key;
    let name = root.to_ascii_lowercase();
    let joint = openzt2_game_data::AssetId::from_key(&name);
    let target = AnimationTargetId::from_name(&Name::new(name));
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
        retired.try_remove::<(FixedAnimationPlayer, RealTimeAnimationPlayer)>();
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

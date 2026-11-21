use bevy::{platform::collections::HashSet, prelude::*};

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

#[derive(Component, Debug, PartialEq, Eq)]
pub(super) struct ManagedAnimationClip {
    pub(super) generation: u64,
    pub(super) node: AnimationNodeIndex,
    pub(super) clip: Handle<AnimationClip>,
    graph: Handle<AnimationGraph>,
}

#[derive(Component)]
pub(super) struct FixedAnimationPlayer;

#[derive(Component)]
pub(super) struct RealTimeAnimationPlayer;

pub(super) fn bind_selected_clips(
    mut commands: Commands,
    sources: AnimationClipSources,
    sets: Res<Assets<AnimationSetAsset>>,
    joints: Query<(Entity, &Transform)>,
    mut changed_sets: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut changed_sources: MessageReader<AssetEvent<AnimationClipSourceAsset>>,
    mut controllers: Query<(
        Entity,
        Ref<AnimationPlaybackController>,
        &mut AnimationPlayer,
        Option<&mut ManagedAnimationClip>,
        Option<&mut AnimationGraphHandle>,
        Option<&mut RetainedAnimationClipSource>,
        Option<&AnimationGraphPlaybackState>,
        Option<&AnimationRigJoints>,
        Has<FixedAnimationPlayer>,
        Has<RealTimeAnimationPlayer>,
        Option<&ActiveAnimationClipOptions>,
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
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    for (
        entity,
        controller,
        mut player,
        existing,
        graph_handle,
        retained,
        state,
        rig,
        fixed,
        real_time,
        options,
    ) in &mut controllers
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
        let wanted = ManagedAnimationClip {
            generation: controller.playback_generation,
            node,
            clip: record.clip.clone(),
            graph: graph.clone(),
        };
        let changed = existing.as_deref() != Some(&wanted) || player.animation(node).is_none();
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
                    for (joint, transform) in joints.iter_many(rig.joints()).matched() {
                        commands
                            .entity(joint)
                            .try_insert(StandardBevyAnimationJointBlendPose {
                                playback_generation: controller.playback_generation,
                                pose: *transform,
                            });
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

        if changed {
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
        if let Some(source) = sources.retained_handle(set, &controller.animation_clip_asset_key) {
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
        let wants_fixed = controller.playback_clock == AnimationPlaybackClock::Simulation;
        if wants_fixed != fixed {
            if wants_fixed {
                commands.entity(entity).try_insert(FixedAnimationPlayer);
            } else {
                commands.entity(entity).try_remove::<FixedAnimationPlayer>();
            }
        }
        if wants_fixed == real_time {
            if wants_fixed {
                commands
                    .entity(entity)
                    .try_remove::<RealTimeAnimationPlayer>();
            } else {
                commands.entity(entity).try_insert(RealTimeAnimationPlayer);
            }
        }
    }
}

pub(super) fn retire_unbound_players(
    mut commands: Commands,
    mut removed: RemovedComponents<AnimationPlaybackController>,
    retired: Query<
        (),
        (
            With<ManagedAnimationClip>,
            Without<AnimationPlaybackController>,
        ),
    >,
) {
    for entity in removed.read().filter(|entity| retired.contains(*entity)) {
        commands.entity(entity).try_remove::<(
            ManagedAnimationClip,
            AnimationPlayer,
            AnimationGraphHandle,
            FixedAnimationPlayer,
            RealTimeAnimationPlayer,
            RetainedAnimationClipSource,
        )>();
    }
}

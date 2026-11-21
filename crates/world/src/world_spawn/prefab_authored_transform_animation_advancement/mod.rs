use bevy::{animation::AnimatedBy, prelude::*};
use openzt2_game_assets::scene_prefab::transform_animation_lowering::{
    PrefabMotion, PrefabMotionRotation,
};
use openzt2_world_state::world_spawn::prefab_authored_rotation_cycle_advancement::PrefabRealTimeRotationClock;
use openzt2_world_state::world_spawn::prefab_authored_transform_animation_advancement::PrefabTransformPlaybackClock;

pub(super) fn insert_prefab_motion(
    entity: &mut EntityCommands<'_>,
    motion: &PrefabMotion,
    rest_rotation: Quat,
) {
    let target = entity.id();
    let mut player = AnimationPlayer::default();
    if let Some((node, instance_origin)) = motion.transform {
        player.start(node).pause();
        entity.insert(if instance_origin {
            PrefabTransformPlaybackClock::Instance(node)
        } else {
            PrefabTransformPlaybackClock::Shared(node)
        });
    }
    if let Some(node) = motion.spin {
        player.start(node).pause();
        entity.insert(PrefabRealTimeRotationClock { node, origin: None });
    }
    if motion.has_rotation {
        entity.insert(PrefabMotionRotation {
            sample: if motion.spin.is_some() {
                Quat::IDENTITY
            } else {
                rest_rotation
            },
            relative_to: motion.spin.map(|_| rest_rotation),
        });
    }
    entity.insert((
        player,
        AnimationGraphHandle(motion.graph.clone()),
        motion.target,
        AnimatedBy(target),
    ));
}

pub(super) fn advance_authored_prefab_transform_animations_from_elapsed_time(
    time: Res<Time>,
    mut players: Query<(Ref<PrefabTransformPlaybackClock>, &mut AnimationPlayer)>,
) {
    for (clock, mut player) in &mut players {
        match *clock {
            PrefabTransformPlaybackClock::Instance(node) if clock.is_added() => {
                if let Some(animation) = player.animation_mut(node) {
                    animation.set_seek_time(-time.delta_secs()).resume();
                }
            }
            PrefabTransformPlaybackClock::Shared(node) => {
                if let Some(animation) = player.animation_mut(node) {
                    animation.set_seek_time(time.elapsed_secs());
                }
            }
            PrefabTransformPlaybackClock::Instance(_) => {}
        }
    }
}

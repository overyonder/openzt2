use crate::animation_playback::player_clock_lanes::PrefabRealTimeAnimation;
use bevy::{animation::AnimatedBy, prelude::*};
use openzt2_game_assets::scene_prefab::transform_animation_lowering::{
    PrefabMotion, PrefabMotionRotation, PrefabTransformClocks,
};

pub(super) fn insert_prefab_motion(
    entity: &mut EntityCommands<'_>,
    motion: &PrefabMotion,
    rest_rotation: Quat,
) {
    let target = entity.id();
    let mut player = AnimationPlayer::default();
    for &(node, _) in motion.transform_clocks.iter() {
        player.start(node).pause();
    }
    entity.insert(PrefabTransformClocks(motion.transform_clocks.clone()));
    if let Some(node) = motion.spin {
        player.start(node).repeat().pause();
        entity.insert(PrefabRealTimeAnimation(node));
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
    mut players: Query<(Ref<PrefabTransformClocks>, &mut AnimationPlayer)>,
) {
    for (clocks, mut player) in &mut players {
        for &(node, instance_origin) in clocks.0.iter() {
            if instance_origin && !clocks.is_added() {
                continue;
            }
            if let Some(animation) = player.animation_mut(node) {
                if instance_origin {
                    animation.set_seek_time(-time.delta_secs()).resume();
                } else {
                    animation.set_seek_time(time.elapsed_secs());
                }
            }
        }
    }
}

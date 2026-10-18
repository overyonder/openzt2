use bevy::animation::{animated_field, AnimationTargetId};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

pub(crate) fn remove_consumed_root_trajectory(
    assets: Res<Assets<AnimationSetAsset>>,
    clips: Res<Assets<AnimationClip>>,
    controllers: Query<(&AnimationPlaybackController, &ConsumedLocomotionRootMotion)>,
    mut joints: Query<(&AnimationJointTarget, &mut Transform)>,
) {
    for (binding, mut transform) in &mut joints {
        let Ok((controller, consumed)) =
            controllers.get(binding.animation_playback_controller_entity)
        else {
            continue;
        };
        if !consumed.owns_playback(controller) {
            continue;
        }
        let Some(record) = assets.get(&controller.animation_set_asset).and_then(|set| {
            set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
        }) else {
            continue;
        };
        let Some(root) = record
            .authored_animation_clip_metadata
            .as_ref()
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
        else {
            continue;
        };
        if !binding
            .joint_asset_key
            .eq_ignore_ascii_case(&root.joint_asset_key)
        {
            continue;
        }
        let Some(clip) = clips.get(&record.animation_clip_handle) else {
            continue;
        };
        let target =
            AnimationTargetId::from_name(&Name::new(root.joint_asset_key.to_ascii_lowercase()));
        // Navigation has consumed the sampled displacement and rotation. Every
        // skin retains the same clip-entry root pose, including after wrapping.
        if let Some(initial) =
            clip.sample_clamped(animated_field!(Transform::translation), target, 0.0)
        {
            transform.translation = initial;
        }
        if let Some(initial) =
            clip.sample_clamped(animated_field!(Transform::rotation), target, 0.0)
        {
            transform.rotation = initial;
        }
    }
}

use bevy::math::Quat;
use openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationRootMotionTransform;

use super::bf_animation_source_types::{BlueFangAnimationNode, BlueFangAnimationProperty};

pub(super) fn calculate_authored_animation_root_motion_transform(
    animation_nodes: &[BlueFangAnimationNode],
) -> AuthoredAnimationRootMotionTransform {
    let root_animation_node = select_authored_animation_motion_root(animation_nodes);
    let translation_track = root_animation_node.and_then(|animation_node| {
        animation_node
            .animation_tracks
            .iter()
            .find(|animation_track| {
                animation_track.animated_property == BlueFangAnimationProperty::Translation
            })
    });
    let translation_from_first_to_last_frame =
        translation_track.map_or([0.0; 3], |animation_track| {
            let first_translation = animation_track
                .keyframe_values
                .get(..3)
                .and_then(|value| value.try_into().ok())
                .unwrap_or([0.0; 3]);
            let last_translation: [f32; 3] = animation_track
                .keyframe_values
                .get(animation_track.keyframe_values.len().saturating_sub(3)..)
                .and_then(|value| value.try_into().ok())
                .unwrap_or(first_translation);
            std::array::from_fn(|axis_index| {
                last_translation[axis_index] - first_translation[axis_index]
            })
        });
    let rotation_track = root_animation_node.and_then(|animation_node| {
        animation_node
            .animation_tracks
            .iter()
            .find(|animation_track| {
                animation_track.animated_property == BlueFangAnimationProperty::Rotation
            })
    });
    let rotation_from_first_to_last_frame =
        rotation_track.map_or([0.0, 0.0, 0.0, 1.0], |animation_track| {
            let first_rotation: [f32; 4] = animation_track
                .keyframe_values
                .get(..4)
                .and_then(|value| value.try_into().ok())
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let last_rotation: [f32; 4] = animation_track
                .keyframe_values
                .get(animation_track.keyframe_values.len().saturating_sub(4)..)
                .and_then(|value| value.try_into().ok())
                .unwrap_or(first_rotation);
            (Quat::from_array(last_rotation) * Quat::from_array(first_rotation).conjugate())
                .to_array()
        });
    AuthoredAnimationRootMotionTransform {
        translation_from_first_to_last_frame,
        rotation_from_first_to_last_frame,
    }
}

/// Selects bip01 by name, falling back to the first track.
pub(super) fn select_authored_animation_motion_root(
    animation_nodes: &[BlueFangAnimationNode],
) -> Option<&BlueFangAnimationNode> {
    animation_nodes
        .iter()
        .find(|node| node.skeleton_joint_name.eq_ignore_ascii_case("bip01"))
        .or_else(|| animation_nodes.first())
}

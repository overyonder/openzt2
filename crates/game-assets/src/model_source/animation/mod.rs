use bevy::{
    animation::{animated_field, animation_curves::AnimatableCurve, AnimationTargetId},
    curve::{FunctionCurve, Interval},
    prelude::*,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::{AuthoredAnimationClipMetadata, AuthoredAnimationMarker},
    animation_text_key::AuthoredAnimationTextKey,
};

use crate::source_coordinate_conversion::{
    convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::{
    bf_animation_keyframe_interpolation::convert_blue_fang_xyz_euler_angles_to_quaternion,
    bf_animation_source_parsing::parse_blue_fang_animation_source_bytes,
    bf_animation_source_types::BlueFangAnimationTrack, conversion_error::ConversionError,
    native_animation_source_lowering::sample_root_motion,
};

pub(super) fn lower_blue_fang_clip(
    path: &str,
    bytes: &[u8],
    sidecar: &[AuthoredAnimationTextKey],
) -> Result<(AnimationClip, AuthoredAnimationClipMetadata), ConversionError> {
    let source = parse_blue_fang_animation_source_bytes(bytes)?;
    let root = source
        .animation_nodes
        .iter()
        .find(|node| node.skeleton_joint_name.eq_ignore_ascii_case("bip01"))
        .or_else(|| source.animation_nodes.first())
        .map(|node| node.skeleton_joint_name.clone());
    let mut clip = AnimationClip::default();
    for node in source.animation_nodes {
        let name = node.skeleton_joint_name.to_ascii_lowercase();
        let target = AnimationTargetId::from_iter([name.as_str()]);
        for track in node.animation_tracks {
            if track.end_seconds().is_none() {
                continue;
            }
            match track {
                BlueFangAnimationTrack::Translation(curve) => clip.add_curve_to_target(target, AnimatableCurve::new(
                    animated_field!(Transform::translation), FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(curve.sample(time)))
                    }),
                )),
                BlueFangAnimationTrack::TranslationAxes(axes) => clip.add_curve_to_target(target, AnimatableCurve::new(
                    animated_field!(Transform::translation), FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                        let vector = axes.each_ref().map(|axis| axis.as_ref().map_or(0.0, |axis| axis.sample(time)[0]));
                        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(vector))
                    }),
                )),
                BlueFangAnimationTrack::Scale(curve) => clip.add_curve_to_target(target, AnimatableCurve::new(
                    animated_field!(Transform::scale), FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                        let [x, y, z] = curve.sample(time);
                        Vec3::new(x, z, y)
                    }),
                )),
                BlueFangAnimationTrack::Rotation(curve) => clip.add_curve_to_target(target, AnimatableCurve::new(
                    animated_field!(Transform::rotation), FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                        Quat::from_array(convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(curve.sample(time).to_array()))
                    }),
                )),
                BlueFangAnimationTrack::Euler(axes) => clip.add_curve_to_target(target, AnimatableCurve::new(
                    animated_field!(Transform::rotation), FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                        let [x, y, z] = axes.each_ref().map(|axis| axis.as_ref().map_or(0.0, |axis| axis.sample(time)[0]));
                        Quat::from_array(convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                            convert_blue_fang_xyz_euler_angles_to_quaternion(x, y, z)
                        ))
                    }),
                )),
            }
        }
    }
    let duration = (source.duration_seconds * 1000.0).max(1.0) as u32;
    clip.set_duration(source.duration_seconds.max(0.001));
    let (root_motion_joint, root_motion_transform) = root.map_or((None, None), |name| {
        let (joint, motion) = sample_root_motion(&clip, &name);
        let normalized = name.to_ascii_lowercase();
        let target = AnimationTargetId::from_iter([normalized.as_str()]);
        let has_translation = clip
            .sample_clamped(animated_field!(Transform::translation), target, 0.0)
            .is_some();
        (has_translation.then_some(joint), Some(motion))
    });
    let authored_animation_text_keys = if sidecar.is_empty() {
        source.animation_markers.iter().filter_map(|marker| {
            crate::source_document::animation_text_keys::parse_blue_fang_embedded_animation_text_key(
                marker.event_time_seconds, &marker.marker_name,
            )
        }).collect()
    } else {
        sidecar.to_vec()
    };
    Ok((
        clip,
        AuthoredAnimationClipMetadata {
            animation_clip_asset_path: path.to_owned(),
            animation_clip_asset_key: path
                .rsplit('/')
                .next()
                .unwrap_or(path)
                .trim_end_matches(".bf")
                .to_owned(),
            duration_milliseconds: duration,
            playback_is_looped: source.authored_flags & 1 != 0,
            additive_blending_is_enabled: source.authored_flags & 4 != 0,
            root_motion_transform,
            root_motion_joint,
            authored_animation_text_keys,
            animation_markers: source
                .animation_markers
                .into_iter()
                .map(|marker| AuthoredAnimationMarker {
                    event_time_milliseconds: (marker.event_time_seconds * 1000.0) as u32,
                    marker_name: marker.marker_name,
                })
                .collect(),
        },
    ))
}

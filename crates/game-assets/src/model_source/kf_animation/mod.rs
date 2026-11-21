use std::collections::BTreeSet;

use bevy::{
    animation::{animated_field, animation_curves::AnimatableCurve, AnimationTargetId},
    curve::{FunctionCurve, Interval},
    prelude::*,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::AuthoredAnimationTextKey,
};

use crate::source_coordinate_conversion::{
    convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::{
    conversion_error::ConversionError,
    kf_animation_curve_sampling::{
        convert_netimmerse_xyz_euler_angles_to_quaternion, convert_wxyz_quaternion_to_xyzw,
        normalize_netimmerse_rotation_quaternion, NetImmerseKeyCurve, NetImmerseSourceKeyframe,
    },
    kf_animation_source_extraction::extract_netimmerse_kf_animation_markers,
    netimmerse_nif_source::{
        animation_controller_source_types::{NetImmerseNiKeyframeData, NetImmerseNiTimeController},
        block_payload::NetImmerseNifBlockPayload,
        document_source_types::NetImmerseNifDocument,
        interpolated_key_source_types::NetImmerseFloatKeyGroup,
    },
};

pub(super) fn lower_netimmerse_kf_clip(
    path: &str,
    document: &NetImmerseNifDocument,
    text_keys: &[AuthoredAnimationTextKey],
) -> Result<(AnimationClip, AuthoredAnimationClipMetadata), ConversionError> {
    let mut sequences = document.blocks().filter_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiControllerSequence(sequence) => Some(sequence),
        _ => None,
    });
    let sequence = sequences.next().ok_or(ConversionError::InvalidSource(
        "KF has no controller sequence",
    ))?;
    if sequences.next().is_some() {
        return Err(ConversionError::InvalidSource(
            "KF has multiple controller sequences",
        ));
    }
    let mut clip = AnimationClip::default();
    let mut targets = BTreeSet::new();
    let mut duration = 0.001_f32;
    let mut repeats = false;
    for joint in &sequence.controlled_blocks {
        let name = joint.target_name.to_ascii_lowercase();
        let target = AnimationTargetId::from_iter([name.as_str()]);
        if !targets.insert(name) {
            return Err(ConversionError::InvalidSource(
                "KF controls one skeleton joint more than once",
            ));
        }
        let controller = match document
            .block(joint.controller_ref)
            .map(|block| &block.payload)
        {
            Some(NetImmerseNifBlockPayload::NiKeyframeController(controller)) => controller,
            _ => {
                return Err(ConversionError::InvalidSource(
                    "KF joint has no keyframe controller",
                ));
            }
        };
        let data = match document
            .block(controller.data_ref)
            .map(|block| &block.payload)
        {
            Some(NetImmerseNifBlockPayload::NiKeyframeData(data)) => data,
            _ => {
                return Err(ConversionError::InvalidSource(
                    "KF controller has no keyframe data",
                ));
            }
        };
        let clock = &controller.controller;
        duration = duration.max(clock.stop_time);
        repeats |= clock.flags & 6 == 0;
        duration = duration.max(add_joint_curves(&mut clip, target, clock, data)?);
    }
    clip.set_duration(duration);
    let root_name = [sequence.accum_root_name.as_str(), "bip01"]
        .into_iter()
        .find_map(|root| {
            sequence
                .controlled_blocks
                .iter()
                .find(|joint| joint.target_name.eq_ignore_ascii_case(root))
        })
        .map(|joint| joint.target_name.as_str());
    let (root_motion_joint, root_motion_transform) = root_name.map_or((None, None), |name| {
        let (joint, motion) =
            super::native_animation_source_lowering::sample_root_motion(&clip, name);
        (Some(joint), Some(motion))
    });
    Ok((
        clip,
        AuthoredAnimationClipMetadata {
            animation_clip_asset_path: path.to_owned(),
            animation_clip_asset_key: sequence.name.clone(),
            duration_milliseconds: (duration * 1000.0).round() as u32,
            playback_is_looped: repeats,
            additive_blending_is_enabled: false,
            root_motion_transform,
            root_motion_joint,
            animation_markers: extract_netimmerse_kf_animation_markers(
                document,
                sequence.text_keys_ref,
            )?,
            authored_animation_text_keys: text_keys.to_vec(),
        },
    ))
}

fn add_joint_curves(
    clip: &mut AnimationClip,
    target: AnimationTargetId,
    controller: &NetImmerseNiTimeController,
    data: &NetImmerseNiKeyframeData,
) -> Result<f32, ConversionError> {
    let clock = KfControllerClock {
        start: controller.start_time,
        stop: controller.stop_time,
        phase: controller.phase,
        frequency: controller.frequency,
        cycle: (controller.flags >> 1) & 3,
        reverse: controller.flags & 0x10 != 0,
    };
    let mut end = 0.0_f32;
    if !data.translations.keys.is_empty() {
        let curve = NetImmerseKeyCurve::new(
            data.translations.interpolation,
            data.translations
                .keys
                .iter()
                .map(|key| NetImmerseSourceKeyframe {
                    time_seconds: key.time,
                    value: key.value,
                    outgoing_bezier_control_value: key.forward,
                    incoming_bezier_control_value: key.backward,
                    tension_bias_continuity: key.tbc,
                })
                .collect(),
        )?;
        end = end.max(curve.end());
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::translation),
                FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                    Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                        curve.sample(clock.time(time)),
                    ))
                }),
            ),
        );
    }
    if !data.scales.keys.is_empty() {
        let curve = float_curve(&data.scales)?;
        end = end.max(curve.end());
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::scale),
                FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                    Vec3::splat(curve.sample(clock.time(time))[0])
                }),
            ),
        );
    }
    if !data.quaternion_keys.is_empty() {
        let curve = NetImmerseKeyCurve::new(
            data.rotation_type,
            data.quaternion_keys
                .iter()
                .map(|key| NetImmerseSourceKeyframe {
                    time_seconds: key.time,
                    value: normalize_netimmerse_rotation_quaternion(
                        convert_wxyz_quaternion_to_xyzw(key.value),
                    ),
                    outgoing_bezier_control_value: key.forward.map(convert_wxyz_quaternion_to_xyzw),
                    incoming_bezier_control_value: key
                        .backward
                        .map(convert_wxyz_quaternion_to_xyzw),
                    tension_bias_continuity: key.tbc,
                })
                .collect(),
        )?;
        end = end.max(curve.end());
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                    let rotation = curve.sample_with(clock.time(time), |left, right, fraction| {
                        Quat::from_array(left)
                            .slerp(Quat::from_array(right), fraction)
                            .to_array()
                    });
                    Quat::from_array(
                        convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                            normalize_netimmerse_rotation_quaternion(rotation),
                        ),
                    )
                }),
            ),
        );
    } else if !data.xyz_rotations.is_empty() {
        let [x, y, z] = data.xyz_rotations.as_slice() else {
            return Err(ConversionError::InvalidSource(
                "KF XYZ rotation does not have three axes",
            ));
        };
        let [x, y, z] = [float_curve(x)?, float_curve(y)?, float_curve(z)?];
        end = end.max(x.end()).max(y.end()).max(z.end());
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                FunctionCurve::new(Interval::EVERYWHERE, move |time| {
                    let time = clock.time(time);
                    Quat::from_array(
                        convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                            convert_netimmerse_xyz_euler_angles_to_quaternion(
                                x.sample(time)[0],
                                y.sample(time)[0],
                                z.sample(time)[0],
                            ),
                        ),
                    )
                }),
            ),
        );
    }
    Ok(end)
}

fn float_curve(group: &NetImmerseFloatKeyGroup) -> Result<NetImmerseKeyCurve<1>, ConversionError> {
    NetImmerseKeyCurve::new(
        group.interpolation,
        group
            .keys
            .iter()
            .map(|key| NetImmerseSourceKeyframe {
                time_seconds: key.time,
                value: [key.value],
                outgoing_bezier_control_value: key.forward.map(|value| [value]),
                incoming_bezier_control_value: key.backward.map(|value| [value]),
                tension_bias_continuity: key.tbc,
            })
            .collect(),
    )
}

#[derive(Clone, Copy)]
struct KfControllerClock {
    start: f32,
    stop: f32,
    phase: f32,
    frequency: f32,
    cycle: u16,
    reverse: bool,
}

impl KfControllerClock {
    fn time(self, elapsed: f32) -> f32 {
        let span = self.stop - self.start;
        if span <= 0.0 {
            return self.start;
        }
        let scaled = if self.reverse {
            (self.stop - elapsed * self.frequency) + self.phase
        } else {
            elapsed.mul_add(self.frequency, self.phase)
        };
        match self.cycle {
            0 if scaled < self.start || scaled > self.stop => {
                self.start + (scaled - self.start).rem_euclid(span)
            }
            1 => {
                let phase = (scaled - self.start).rem_euclid(2.0 * span);
                self.start + phase.min(2.0 * span - phase)
            }
            _ => scaled.clamp(self.start, self.stop),
        }
    }
}

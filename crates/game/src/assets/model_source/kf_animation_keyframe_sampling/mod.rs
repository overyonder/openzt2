//! NetImmerse translation, rotation, and scale keyframe decoding.

use crate::assets::source_coordinate_conversion::{
    convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::{
    conversion_error::ConversionError,
    kf_animation_curve_sampling::{
        convert_netimmerse_xyz_euler_angles_to_quaternion, convert_wxyz_quaternion_to_xyzw,
        normalize_netimmerse_rotation_quaternion, sample_netimmerse_keyframes,
        sample_netimmerse_linear_scalar_keyframes, NetImmerseSourceKeyframe,
    },
    kf_animation_source_types::{NetImmerseKfAnimationChannel, SampledNetImmerseKfAnimationTrack},
    netimmerse_nif_source::{
        animation_controller_source_types::NetImmerseNiKeyframeData,
        interpolated_key_source_types::{
            NetImmerseFloatKeyGroup, NetImmerseQuaternionKey, NetImmerseVector3KeyGroup,
        },
    },
};

pub(super) fn append_sampled_netimmerse_translation_track(
    skeleton_joint_node_index: usize,
    translation_key_group: &NetImmerseVector3KeyGroup,
    sampled_animation_tracks: &mut Vec<SampledNetImmerseKfAnimationTrack>,
) -> Result<(), ConversionError> {
    let sampled_keyframes: Vec<_> = sample_netimmerse_keyframes(
        translation_key_group.interpolation,
        &translation_key_group
            .keys
            .iter()
            .map(|source_keyframe| NetImmerseSourceKeyframe {
                time_seconds: source_keyframe.time,
                value: source_keyframe.value,
                outgoing_bezier_control_value: source_keyframe.forward,
                incoming_bezier_control_value: source_keyframe.backward,
                tension_bias_continuity: source_keyframe.tbc,
            })
            .collect::<Vec<_>>(),
    )?
    .into_iter()
    .map(|(time, translation)| {
        (
            time,
            convert_source_z_up_vector_to_bevy_y_up_coordinates(translation),
        )
    })
    .collect();
    append_sampled_netimmerse_animation_track(
        skeleton_joint_node_index,
        NetImmerseKfAnimationChannel::Translation,
        sampled_keyframes,
        translation_key_group.interpolation,
        sampled_animation_tracks,
    )
}

pub(super) fn append_sampled_netimmerse_scale_track(
    skeleton_joint_node_index: usize,
    scale_key_group: &NetImmerseFloatKeyGroup,
    sampled_animation_tracks: &mut Vec<SampledNetImmerseKfAnimationTrack>,
) -> Result<(), ConversionError> {
    let netimmerse_source_keyframes = scale_key_group
        .keys
        .iter()
        .map(|source_keyframe| NetImmerseSourceKeyframe {
            time_seconds: source_keyframe.time,
            value: [source_keyframe.value; 3],
            outgoing_bezier_control_value: source_keyframe
                .forward
                .map(|source_control_value| [source_control_value; 3]),
            incoming_bezier_control_value: source_keyframe
                .backward
                .map(|source_control_value| [source_control_value; 3]),
            tension_bias_continuity: source_keyframe.tbc,
        })
        .collect::<Vec<_>>();
    append_sampled_netimmerse_animation_track(
        skeleton_joint_node_index,
        NetImmerseKfAnimationChannel::Scale,
        sample_netimmerse_keyframes(scale_key_group.interpolation, &netimmerse_source_keyframes)?,
        scale_key_group.interpolation,
        sampled_animation_tracks,
    )
}

pub(super) fn append_sampled_netimmerse_rotation_track(
    skeleton_joint_node_index: usize,
    keyframe_data: &NetImmerseNiKeyframeData,
    sampled_animation_tracks: &mut Vec<SampledNetImmerseKfAnimationTrack>,
) -> Result<(), ConversionError> {
    if !keyframe_data.quaternion_keys.is_empty() {
        let sampled_keyframes: Vec<_> = sample_netimmerse_quaternion_keyframes(
            keyframe_data.rotation_type,
            &keyframe_data.quaternion_keys,
        )?
        .into_iter()
        .map(|(time, rotation)| {
            (
                time,
                convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                    rotation,
                ),
            )
        })
        .collect();
        return append_sampled_netimmerse_animation_track(
            skeleton_joint_node_index,
            NetImmerseKfAnimationChannel::Rotation,
            sampled_keyframes,
            keyframe_data.rotation_type,
            sampled_animation_tracks,
        );
    }
    if keyframe_data.xyz_rotations.is_empty() {
        return Ok(());
    }
    if keyframe_data.xyz_rotations.len() != 3 {
        return Err(ConversionError::InvalidSource(
            "KF XYZ rotation does not have three axes",
        ));
    }
    let sampled_euler_axis_keyframes = keyframe_data
        .xyz_rotations
        .iter()
        .map(sample_netimmerse_float_key_group)
        .collect::<Result<Vec<_>, _>>()?;
    let mut keyframe_times_seconds = sampled_euler_axis_keyframes
        .iter()
        .flat_map(|axis_keyframes| {
            axis_keyframes
                .iter()
                .map(|(keyframe_time_seconds, _)| *keyframe_time_seconds)
        })
        .collect::<Vec<_>>();
    keyframe_times_seconds.sort_by(f32::total_cmp);
    keyframe_times_seconds
        .dedup_by(|left_time, right_time| left_time.to_bits() == right_time.to_bits());
    let sampled_keyframes = keyframe_times_seconds
        .into_iter()
        .map(|keyframe_time_seconds| {
            let x_axis_radians = sample_netimmerse_linear_scalar_keyframes(
                &sampled_euler_axis_keyframes[0],
                keyframe_time_seconds,
            );
            let y_axis_radians = sample_netimmerse_linear_scalar_keyframes(
                &sampled_euler_axis_keyframes[1],
                keyframe_time_seconds,
            );
            let z_axis_radians = sample_netimmerse_linear_scalar_keyframes(
                &sampled_euler_axis_keyframes[2],
                keyframe_time_seconds,
            );
            (
                keyframe_time_seconds,
                convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                    convert_netimmerse_xyz_euler_angles_to_quaternion(
                        x_axis_radians,
                        y_axis_radians,
                        z_axis_radians,
                    ),
                ),
            )
        })
        .collect();
    append_sampled_netimmerse_animation_track(
        skeleton_joint_node_index,
        NetImmerseKfAnimationChannel::Rotation,
        sampled_keyframes,
        Some(1),
        sampled_animation_tracks,
    )
}

fn append_sampled_netimmerse_animation_track<const N: usize>(
    skeleton_joint_node_index: usize,
    animation_channel: NetImmerseKfAnimationChannel,
    sampled_keyframes: Vec<(f32, [f32; N])>,
    source_interpolation_kind: Option<u32>,
    sampled_animation_tracks: &mut Vec<SampledNetImmerseKfAnimationTrack>,
) -> Result<(), ConversionError> {
    if sampled_keyframes.is_empty() {
        return Ok(());
    }
    if sampled_keyframes
        .windows(2)
        .any(|adjacent_keyframe_pair| adjacent_keyframe_pair[0].0 > adjacent_keyframe_pair[1].0)
    {
        return Err(ConversionError::InvalidValue(
            "KF key times are not monotonic",
        ));
    }
    sampled_animation_tracks.push(SampledNetImmerseKfAnimationTrack {
        skeleton_joint_node_index,
        animation_channel,
        keyframe_times_seconds: sampled_keyframes
            .iter()
            .map(|(keyframe_time_seconds, _)| *keyframe_time_seconds)
            .collect(),
        keyframe_values: sampled_keyframes
            .into_iter()
            .flat_map(|(_, keyframe_value)| keyframe_value)
            .collect(),
        gltf_interpolation_mode: if source_interpolation_kind == Some(5) {
            "STEP"
        } else {
            "LINEAR"
        },
    });
    Ok(())
}

fn sample_netimmerse_float_key_group(
    float_key_group: &NetImmerseFloatKeyGroup,
) -> Result<Vec<(f32, f32)>, ConversionError> {
    let netimmerse_source_keyframes = float_key_group
        .keys
        .iter()
        .map(|source_keyframe| NetImmerseSourceKeyframe {
            time_seconds: source_keyframe.time,
            value: [source_keyframe.value],
            outgoing_bezier_control_value: source_keyframe
                .forward
                .map(|source_control_value| [source_control_value]),
            incoming_bezier_control_value: source_keyframe
                .backward
                .map(|source_control_value| [source_control_value]),
            tension_bias_continuity: source_keyframe.tbc,
        })
        .collect::<Vec<_>>();
    Ok(
        sample_netimmerse_keyframes(float_key_group.interpolation, &netimmerse_source_keyframes)?
            .into_iter()
            .map(|(time_seconds, sampled_value)| (time_seconds, sampled_value[0]))
            .collect(),
    )
}

fn sample_netimmerse_quaternion_keyframes(
    source_interpolation_kind: Option<u32>,
    quaternion_keyframes: &[NetImmerseQuaternionKey],
) -> Result<Vec<(f32, [f32; 4])>, ConversionError> {
    let netimmerse_source_keyframes = quaternion_keyframes
        .iter()
        .map(|source_keyframe| NetImmerseSourceKeyframe {
            time_seconds: source_keyframe.time,
            value: convert_wxyz_quaternion_to_xyzw(source_keyframe.value),
            outgoing_bezier_control_value: source_keyframe
                .forward
                .map(convert_wxyz_quaternion_to_xyzw),
            incoming_bezier_control_value: source_keyframe
                .backward
                .map(convert_wxyz_quaternion_to_xyzw),
            tension_bias_continuity: source_keyframe.tbc,
        })
        .collect::<Vec<_>>();
    Ok(
        sample_netimmerse_keyframes(source_interpolation_kind, &netimmerse_source_keyframes)?
            .into_iter()
            .map(|(time_seconds, rotation_quaternion)| {
                (
                    time_seconds,
                    normalize_netimmerse_rotation_quaternion(rotation_quaternion),
                )
            })
            .collect(),
    )
}

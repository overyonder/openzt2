use gltf::json::{
    accessor::Type,
    animation::{Animation, Channel, Interpolation, Property, Sampler, Target},
    scene::{Node, Scene},
    validation::Checked,
    Index, Root,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::AuthoredAnimationTextKey,
};
use serde_json::json;

use super::{
    conversion_error::ConversionError,
    gltf_binary_buffer::GltfBinaryBuffer,
    kf_animation_source_types::{NetImmerseKfAnimationChannel, ParsedNetImmerseKfAnimation},
};

pub(super) fn lower_parsed_netimmerse_kf_animation_to_glb_and_authored_clip_metadata(
    animation_clip_asset_path: &str,
    parsed_netimmerse_kf_animation: ParsedNetImmerseKfAnimation,
    authored_animation_text_keys: &[AuthoredAnimationTextKey],
) -> Result<(Vec<u8>, AuthoredAnimationClipMetadata), ConversionError> {
    let mut gltf_root = Root::default();
    let mut binary_buffer = GltfBinaryBuffer::default();
    gltf_root.nodes = parsed_netimmerse_kf_animation
        .skeleton_joint_names
        .iter()
        .map(|skeleton_joint_name| Node {
            name: Some(skeleton_joint_name.clone()),
            ..Default::default()
        })
        .collect();
    gltf_root.scene = Some(Index::new(0));
    gltf_root.scenes.push(Scene {
        extensions: None,
        extras: None,
        name: None,
        nodes: (0..gltf_root.nodes.len())
            .map(|node_index| Index::new(node_index as u32))
            .collect(),
    });

    let mut animation_samplers = Vec::new();
    let mut animation_channels = Vec::new();
    for sampled_animation_track in &parsed_netimmerse_kf_animation.sampled_animation_tracks {
        let input_accessor = binary_buffer.add_f32_accessor(
            &mut gltf_root,
            &sampled_animation_track.keyframe_times_seconds,
            Type::Scalar,
            true,
            None,
        );
        let output_accessor = binary_buffer.add_f32_accessor(
            &mut gltf_root,
            &sampled_animation_track.keyframe_values,
            animation_channel_accessor_type(sampled_animation_track.animation_channel),
            false,
            None,
        );
        let animation_sampler_index = animation_samplers.len();
        animation_samplers.push(Sampler {
            extensions: None,
            extras: None,
            input: input_accessor,
            interpolation: Checked::Valid(gltf_animation_interpolation(
                sampled_animation_track.gltf_interpolation_mode,
            )?),
            output: output_accessor,
        });
        animation_channels.push(Channel {
            sampler: Index::new(animation_sampler_index as u32),
            target: Target {
                extensions: None,
                extras: None,
                node: Index::new(sampled_animation_track.skeleton_joint_node_index as u32),
                path: Checked::Valid(gltf_animation_property(
                    sampled_animation_track.animation_channel,
                )),
            },
            extensions: None,
            extras: None,
        });
    }
    let animation_extras = serde_json::value::to_raw_value(&json!({
        "openzt2DurationSeconds": parsed_netimmerse_kf_animation.duration_seconds,
        "openzt2Loop": parsed_netimmerse_kf_animation.playback_is_looped,
        "openzt2AccumulationRoot": parsed_netimmerse_kf_animation.accumulation_root_name,
        "openzt2Markers": parsed_netimmerse_kf_animation
            .animation_markers
            .iter()
            .map(|animation_marker| json!({
                "time": animation_marker.event_time_milliseconds as f32 / 1000.0,
                "name": animation_marker.marker_name,
            }))
            .collect::<Vec<_>>(),
    }))?;
    gltf_root.animations.push(Animation {
        extensions: None,
        extras: Some(animation_extras),
        channels: animation_channels,
        name: Some(
            parsed_netimmerse_kf_animation
                .animation_clip_asset_key
                .clone(),
        ),
        samplers: animation_samplers,
    });

    let authored_animation_clip_metadata = AuthoredAnimationClipMetadata {
        animation_clip_asset_path: animation_clip_asset_path.to_owned(),
        animation_clip_asset_key: parsed_netimmerse_kf_animation.animation_clip_asset_key,
        duration_milliseconds: ((parsed_netimmerse_kf_animation.duration_seconds.max(0.0) * 1000.0)
            .round()
            .min(u32::MAX as f32) as u32)
            .max(1),
        playback_is_looped: parsed_netimmerse_kf_animation.playback_is_looped,
        additive_blending_is_enabled: false,
        root_motion_transform: None,
        root_motion_joint: None,
        animation_markers: parsed_netimmerse_kf_animation.animation_markers,
        authored_animation_text_keys: authored_animation_text_keys.to_vec(),
    };

    Ok((
        binary_buffer.finish(gltf_root)?,
        authored_animation_clip_metadata,
    ))
}

fn gltf_animation_interpolation(
    interpolation_mode: &str,
) -> Result<Interpolation, ConversionError> {
    match interpolation_mode {
        "LINEAR" => Ok(Interpolation::Linear),
        "STEP" => Ok(Interpolation::Step),
        "CUBICSPLINE" => Ok(Interpolation::CubicSpline),
        _ => Err(ConversionError::InvalidValue(
            "unsupported KF animation interpolation mode",
        )),
    }
}

const fn gltf_animation_property(animation_channel: NetImmerseKfAnimationChannel) -> Property {
    match animation_channel {
        NetImmerseKfAnimationChannel::Translation => Property::Translation,
        NetImmerseKfAnimationChannel::Rotation => Property::Rotation,
        NetImmerseKfAnimationChannel::Scale => Property::Scale,
    }
}

const fn animation_channel_accessor_type(animation_channel: NetImmerseKfAnimationChannel) -> Type {
    match animation_channel {
        NetImmerseKfAnimationChannel::Translation | NetImmerseKfAnimationChannel::Scale => {
            Type::Vec3
        }
        NetImmerseKfAnimationChannel::Rotation => Type::Vec4,
    }
}

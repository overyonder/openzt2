use gltf::json::{
    accessor::Type,
    animation::{Animation, Channel, Interpolation, Property, Sampler, Target},
    scene::{Node, Scene},
    validation::Checked,
    Index, Root,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::{AuthoredAnimationClipMetadata, AuthoredAnimationMarker},
    animation_text_key::AuthoredAnimationTextKey,
};
use serde_json::json;

use crate::assets::source_coordinate_conversion::{
    convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::{
    bf_animation_root_motion_calculation::{
        calculate_authored_animation_root_motion_transform, select_authored_animation_motion_root,
    },
    bf_animation_source_types::{BlueFangAnimationProperty, ParsedBlueFangAnimation},
    conversion_error::ConversionError,
    gltf_binary_buffer::GltfBinaryBuffer,
};

pub(super) fn lower_parsed_blue_fang_animation_to_glb_and_authored_clip_metadata(
    animation_clip_asset_path: &str,
    mut parsed_blue_fang_animation: ParsedBlueFangAnimation,
    authored_animation_text_keys: &[AuthoredAnimationTextKey],
) -> Result<(Vec<u8>, AuthoredAnimationClipMetadata), ConversionError> {
    convert_blue_fang_animation_tracks_to_bevy_coordinates(&mut parsed_blue_fang_animation);
    let animation_clip_asset_key = animation_clip_asset_path
        .rsplit('/')
        .next()
        .unwrap_or(animation_clip_asset_path)
        .trim_end_matches(".bf");
    let mut gltf_root = Root::default();
    let mut binary_buffer = GltfBinaryBuffer::default();
    gltf_root.nodes = parsed_blue_fang_animation
        .animation_nodes
        .iter()
        .map(|animation_node| Node {
            name: Some(animation_node.skeleton_joint_name.clone()),
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

    let mut animation_channels = Vec::new();
    let mut animation_samplers = Vec::new();
    for (animation_node_index, animation_node) in parsed_blue_fang_animation
        .animation_nodes
        .iter()
        .enumerate()
    {
        for animation_track in &animation_node.animation_tracks {
            let input_accessor = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &animation_track.keyframe_times_seconds,
                Type::Scalar,
                true,
                None,
            );
            let output_accessor = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &animation_track.keyframe_values,
                animation_property_accessor_type(animation_track.animated_property),
                false,
                None,
            );
            let animation_sampler_index = animation_samplers.len();
            animation_samplers.push(Sampler {
                extensions: None,
                extras: None,
                input: input_accessor,
                interpolation: Checked::Valid(Interpolation::Linear),
                output: output_accessor,
            });
            animation_channels.push(Channel {
                sampler: Index::new(animation_sampler_index as u32),
                target: Target {
                    extensions: None,
                    extras: None,
                    node: Index::new(animation_node_index as u32),
                    path: Checked::Valid(gltf_animation_property(
                        animation_track.animated_property,
                    )),
                },
                extensions: None,
                extras: None,
            });
        }
    }
    let animation_extras = serde_json::value::to_raw_value(&json!({
        "openzt2DurationSeconds": parsed_blue_fang_animation.duration_seconds,
        "openzt2Loop": parsed_blue_fang_animation.authored_flags & 1 != 0,
        "openzt2RootMotion": parsed_blue_fang_animation.authored_flags & 2 != 0,
        "openzt2Additive": parsed_blue_fang_animation.authored_flags & 4 != 0,
        "openzt2Markers": parsed_blue_fang_animation
            .animation_markers
            .iter()
            .map(|animation_marker| json!({
                "time": animation_marker.event_time_seconds,
                "name": animation_marker.marker_name,
            }))
            .collect::<Vec<_>>(),
    }))?;
    gltf_root.animations.push(Animation {
        extensions: None,
        extras: Some(animation_extras),
        channels: animation_channels,
        name: Some(animation_clip_asset_key.to_owned()),
        samplers: animation_samplers,
    });

    // Clip displacement comes from the loaded tracks, independently of BF header flags.
    let root_motion_transform = select_authored_animation_motion_root(
        &parsed_blue_fang_animation.animation_nodes,
    )
    .map(|_| {
        calculate_authored_animation_root_motion_transform(
            &parsed_blue_fang_animation.animation_nodes,
        )
    });
    let authored_animation_clip_metadata = AuthoredAnimationClipMetadata {
        animation_clip_asset_path: animation_clip_asset_path.to_owned(),
        animation_clip_asset_key: animation_clip_asset_key.to_owned(),
        duration_milliseconds: (parsed_blue_fang_animation.duration_seconds * 1000.0).max(1.0)
            as u32,
        playback_is_looped: parsed_blue_fang_animation.authored_flags & 1 != 0,
        additive_blending_is_enabled: parsed_blue_fang_animation.authored_flags & 4 != 0,
        root_motion_transform,
        root_motion_joint: root_motion_transform.and_then(|_| {
            let root = select_authored_animation_motion_root(&parsed_blue_fang_animation.animation_nodes)?;
            let translation = root.animation_tracks.iter().find(|track| {
                track.animated_property == super::bf_animation_source_types::BlueFangAnimationProperty::Translation
            })?;
            Some(openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationRootMotionJoint {
                joint_asset_key: root.skeleton_joint_name.clone(),
                initial_translation: translation.keyframe_values.get(..3)?.try_into().ok()?,
            })
        }),
        animation_markers: parsed_blue_fang_animation
            .animation_markers
            .into_iter()
            .map(|animation_marker| AuthoredAnimationMarker {
                event_time_milliseconds: (animation_marker.event_time_seconds * 1000.0) as u32,
                marker_name: animation_marker.marker_name,
            })
            .collect(),
        authored_animation_text_keys: authored_animation_text_keys.to_vec(),
    };

    Ok((
        binary_buffer.finish(gltf_root)?,
        authored_animation_clip_metadata,
    ))
}

fn convert_blue_fang_animation_tracks_to_bevy_coordinates(
    parsed_blue_fang_animation: &mut ParsedBlueFangAnimation,
) {
    parsed_blue_fang_animation
        .animation_nodes
        .iter_mut()
        .flat_map(|animation_node| &mut animation_node.animation_tracks)
        .for_each(|animation_track| match animation_track.animated_property {
            BlueFangAnimationProperty::Translation => animation_track
                .keyframe_values
                .chunks_exact_mut(3)
                .for_each(|translation| {
                    translation.copy_from_slice(
                        &convert_source_z_up_vector_to_bevy_y_up_coordinates([
                            translation[0],
                            translation[1],
                            translation[2],
                        ]),
                    );
                }),
            BlueFangAnimationProperty::Rotation => animation_track
                .keyframe_values
                .chunks_exact_mut(4)
                .for_each(|rotation| {
                    rotation.copy_from_slice(
                        &convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                            [rotation[0], rotation[1], rotation[2], rotation[3]],
                        ),
                    );
                }),
            BlueFangAnimationProperty::Scale => animation_track
                .keyframe_values
                .chunks_exact_mut(3)
                .for_each(|scale| scale.swap(1, 2)),
        });
}

const fn gltf_animation_property(animation_property: BlueFangAnimationProperty) -> Property {
    match animation_property {
        BlueFangAnimationProperty::Translation => Property::Translation,
        BlueFangAnimationProperty::Rotation => Property::Rotation,
        BlueFangAnimationProperty::Scale => Property::Scale,
    }
}

const fn animation_property_accessor_type(animation_property: BlueFangAnimationProperty) -> Type {
    match animation_property {
        BlueFangAnimationProperty::Rotation => Type::Vec4,
        BlueFangAnimationProperty::Translation | BlueFangAnimationProperty::Scale => Type::Vec3,
    }
}

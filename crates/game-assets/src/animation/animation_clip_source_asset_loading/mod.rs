use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::AuthoredAnimationTextKey,
};
use std::{io, sync::Arc};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use crate::{
    model_source::native_animation_source_lowering::{
        kf_sequence_label, lower_native_animation_source,
    },
    source_document::animation_text_keys::parse_blue_fang_animation_text_key_sidecar_source,
};

use super::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationTimelineCommand, AnimationTimelineKey,
    AuthoredAnimationKeyframeEvent,
};

#[derive(Default, TypePath)]
pub(super) struct AnimationClipSourceAssetLoader;

impl AssetLoader for AnimationClipSourceAssetLoader {
    type Asset = AnimationClipSourceAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_animation_clip", skip_all)
    )]
    async fn load(
        &self,
        animation_clip_source_reader: &mut dyn Reader,
        _animation_clip_loader_settings: &Self::Settings,
        animation_clip_load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut animation_source_bytes = Vec::new();
        animation_clip_source_reader
            .read_to_end(&mut animation_source_bytes)
            .await?;
        let animation_source_asset_path = animation_clip_load_context
            .path()
            .path()
            .to_string_lossy()
            .replace('\\', "/");
        let text_key_sidecar_asset_path =
            std::path::Path::new(&animation_source_asset_path).with_extension("txtkeys");
        let authored_animation_text_keys = match animation_clip_load_context
            .read_asset_bytes(text_key_sidecar_asset_path.clone())
            .await
        {
            Ok(text_key_sidecar_source_bytes) => parse_blue_fang_animation_text_key_sidecar_source(
                &text_key_sidecar_asset_path.to_string_lossy(),
                &text_key_sidecar_source_bytes,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(bevy::asset::ReadAssetBytesError::AssetReaderError(
                bevy::asset::io::AssetReaderError::NotFound(_),
            )) => Vec::new(),
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        let mut clips = lower_native_animation_source(
            &animation_source_asset_path,
            &animation_source_bytes,
            &authored_animation_text_keys,
        )
        .map_err(io::Error::other)?
        .into_iter();
        let (default_index, clip, metadata) = clips
            .next()
            .ok_or_else(|| io::Error::other("animation asset contains no clips"))?;
        let default =
            prepare_animation_clip_source(clip, metadata, "", animation_clip_load_context);
        animation_clip_load_context
            .add_labeled_asset(kf_sequence_label(default_index), default.clone());
        for (index, clip, metadata) in clips {
            let label = kf_sequence_label(index);
            let source = prepare_animation_clip_source(
                clip,
                metadata,
                &format!("{label}/"),
                animation_clip_load_context,
            );
            animation_clip_load_context.add_labeled_asset(label, source);
        }
        Ok(default)
    }

    fn extensions(&self) -> &[&str] {
        &["bf", "kf"]
    }
}

fn prepare_animation_clip_source(
    mut clip: AnimationClip,
    mut authored_animation_clip_metadata: AuthoredAnimationClipMetadata,
    label_prefix: &str,
    animation_clip_load_context: &mut LoadContext<'_>,
) -> AnimationClipSourceAsset {
    let marker_time = |name: &str| {
        authored_animation_clip_metadata
            .animation_markers
            .iter()
            .find(|marker| marker.marker_name.eq_ignore_ascii_case(name))
            .map(|marker| marker.event_time_milliseconds as f32 / 1000.0)
    };
    let loop_bounds = marker_time("loop_start")
        .zip(marker_time("loop_end"))
        .filter(|(start, end)| start < end && *end <= clip.duration())
        .map(|(start, end)| start..end);
    let duration = authored_animation_clip_metadata
        .duration_milliseconds
        .max(1);
    let mut timeline =
        std::mem::take(&mut authored_animation_clip_metadata.authored_animation_text_keys)
            .into_iter()
            .filter(|key| !key.animation_text_commands.is_empty())
            .map(|key| AnimationTimelineKey {
                milliseconds: text_key_time_milliseconds(&key, duration),
                commands: key
                    .animation_text_commands
                    .into_iter()
                    .map(|command| AnimationTimelineCommand {
                        joint: command.target_skeleton_joint_asset_key.map(|joint| {
                            openzt2_game_data::AssetId::from_key(&joint.to_ascii_lowercase())
                        }),
                        action: Arc::new(command.animation_text_action),
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            })
            .collect::<Vec<_>>();
    timeline.sort_by_key(|key| key.milliseconds);
    let timeline: Arc<[_]> = timeline.into();
    if let Some(last) = timeline.last() {
        clip.set_duration(clip.duration().max(last.milliseconds as f32 / 1000.0));
    }
    let mut first_key = 0;
    while first_key < timeline.len() {
        let time = forward_bevy_event_time(timeline[first_key].milliseconds);
        let end_key = first_key
            + timeline[first_key..]
                .partition_point(|key| forward_bevy_event_time(key.milliseconds) == time);
        let keys = timeline.clone();
        clip.add_event_fn(time, move |commands, entity, _, _| {
            commands.trigger(AuthoredAnimationKeyframeEvent {
                entity,
                timeline: keys.clone(),
                first_key,
                end_key,
            });
        });
        first_key = end_key;
    }
    let animation_clip_handle =
        animation_clip_load_context.add_labeled_asset(format!("{label_prefix}Clip"), clip);
    let (graph, animation_node) = AnimationGraph::from_clip(animation_clip_handle.clone());
    let animation_graph =
        animation_clip_load_context.add_labeled_asset(format!("{label_prefix}Graph"), graph);
    AnimationClipSourceAsset {
        authored_animation_clip_metadata: Arc::new(authored_animation_clip_metadata),
        timeline,
        animation_clip_handle,
        animation_graph,
        animation_node,
        loop_bounds,
    }
}

fn forward_bevy_event_time(milliseconds: u32) -> f32 {
    if milliseconds == 0 {
        return 0.0;
    }
    // Bevy's forward events exclude the destination. Use the last representable
    // time below the inclusive key boundary, including seek-time rounding.
    let boundary = (f64::from(milliseconds) - 0.0001) / 1000.0;
    let rounded = boundary as f32;
    if f64::from(rounded) < boundary {
        rounded
    } else {
        rounded.next_down()
    }
}

fn text_key_time_milliseconds(key: &AuthoredAnimationTextKey, duration_milliseconds: u32) -> u32 {
    key.authored_time_seconds
        .map(|seconds| (seconds.max(0.0) * 1000.0).round() as u32)
        .or_else(|| {
            key.authored_frame_number
                .map(|frame| (frame.max(0.0) * 1000.0 / 30.0).round() as u32)
        })
        .or_else(|| {
            key.authored_playback_percentage.map(|percent| {
                let fraction = if percent > 1.0 {
                    percent / 100.0
                } else {
                    percent
                };
                (fraction.clamp(0.0, 1.0) * duration_milliseconds as f32).round() as u32
            })
        })
        .unwrap_or(0)
}

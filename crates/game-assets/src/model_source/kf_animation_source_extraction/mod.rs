use std::collections::BTreeSet;

use openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationMarker;

use super::{
    conversion_error::ConversionError,
    kf_animation_keyframe_sampling::{
        append_sampled_netimmerse_rotation_track, append_sampled_netimmerse_scale_track,
        append_sampled_netimmerse_translation_track,
    },
    kf_animation_source_types::ParsedNetImmerseKfAnimation,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};

pub(super) fn extract_netimmerse_kf_animation_from_document(
    netimmerse_kf_document: &NetImmerseNifDocument,
) -> Result<ParsedNetImmerseKfAnimation, ConversionError> {
    let mut controller_sequences = netimmerse_kf_document.blocks().filter_map(|block| {
        let NetImmerseNifBlockPayload::NiControllerSequence(controller_sequence) = &block.payload
        else {
            return None;
        };
        Some(controller_sequence)
    });
    let controller_sequence = controller_sequences
        .next()
        .ok_or(ConversionError::InvalidSource(
            "KF has no controller sequence",
        ))?;
    if controller_sequences.next().is_some() {
        return Err(ConversionError::InvalidSource(
            "KF has multiple controller sequences",
        ));
    }

    let mut skeleton_joint_names = Vec::new();
    let mut claimed_case_folded_skeleton_joint_names = BTreeSet::new();
    let mut sampled_animation_tracks = Vec::new();
    let mut duration_seconds = 0.0_f32;
    let mut playback_is_looped = false;
    for controlled_skeleton_joint in &controller_sequence.controlled_blocks {
        let case_folded_skeleton_joint_name =
            controlled_skeleton_joint.target_name.to_ascii_lowercase();
        if !claimed_case_folded_skeleton_joint_names.insert(case_folded_skeleton_joint_name) {
            return Err(ConversionError::InvalidSource(
                "KF controls one skeleton joint more than once",
            ));
        }
        let skeleton_joint_node_index = skeleton_joint_names.len();
        skeleton_joint_names.push(controlled_skeleton_joint.target_name.clone());
        let keyframe_controller = netimmerse_kf_document
            .block(controlled_skeleton_joint.controller_ref)
            .and_then(|block| match &block.payload {
                NetImmerseNifBlockPayload::NiKeyframeController(keyframe_controller) => {
                    Some(keyframe_controller)
                }
                _ => None,
            })
            .ok_or(ConversionError::InvalidSource(
                "KF controlled joint does not reference a keyframe controller",
            ))?;
        if keyframe_controller.controller.flags & 0x0010 != 0 {
            return Err(ConversionError::InvalidSource(
                "backwards KF controllers require an explicit playback mapping",
            ));
        }
        playback_is_looped |= keyframe_controller.controller.flags & 0x0006 == 0;
        duration_seconds = duration_seconds.max(keyframe_controller.controller.stop_time.max(0.0));
        let keyframe_data = netimmerse_kf_document
            .block(keyframe_controller.data_ref)
            .and_then(|block| match &block.payload {
                NetImmerseNifBlockPayload::NiKeyframeData(keyframe_data) => Some(keyframe_data),
                _ => None,
            })
            .ok_or(ConversionError::InvalidSource(
                "KF controller does not reference keyframe data",
            ))?;
        append_sampled_netimmerse_translation_track(
            skeleton_joint_node_index,
            &keyframe_data.translations,
            &mut sampled_animation_tracks,
        )?;
        append_sampled_netimmerse_scale_track(
            skeleton_joint_node_index,
            &keyframe_data.scales,
            &mut sampled_animation_tracks,
        )?;
        append_sampled_netimmerse_rotation_track(
            skeleton_joint_node_index,
            keyframe_data,
            &mut sampled_animation_tracks,
        )?;
    }
    if sampled_animation_tracks.is_empty() {
        return Err(ConversionError::InvalidSource("KF has no transform tracks"));
    }
    duration_seconds = sampled_animation_tracks
        .iter()
        .flat_map(|animation_track| animation_track.keyframe_times_seconds.iter().copied())
        .fold(duration_seconds, f32::max);
    let animation_markers = extract_netimmerse_kf_animation_markers(
        netimmerse_kf_document,
        controller_sequence.text_keys_ref,
    )?;

    Ok(ParsedNetImmerseKfAnimation {
        animation_clip_asset_key: controller_sequence.name.clone(),
        skeleton_joint_names,
        sampled_animation_tracks,
        duration_seconds,
        playback_is_looped,
        accumulation_root_name: controller_sequence.accum_root_name.clone(),
        animation_markers,
    })
}

fn extract_netimmerse_kf_animation_markers(
    netimmerse_kf_document: &NetImmerseNifDocument,
    text_key_block_reference: i32,
) -> Result<Vec<AuthoredAnimationMarker>, ConversionError> {
    let animation_markers = match netimmerse_kf_document
        .block(text_key_block_reference)
        .map(|block| &block.payload)
    {
        Some(NetImmerseNifBlockPayload::NiTextKeyExtraData(text_key_data)) => text_key_data
            .keys
            .iter()
            .map(|text_key| AuthoredAnimationMarker {
                event_time_milliseconds: (text_key.time.max(0.0) * 1000.0)
                    .round()
                    .min(u32::MAX as f32) as u32,
                marker_name: text_key.value.trim_end_matches('\0').trim().to_owned(),
            })
            .collect::<Vec<_>>(),
        None if text_key_block_reference < 0 => Vec::new(),
        _ => {
            return Err(ConversionError::InvalidSource(
                "KF has an invalid text-key reference",
            ));
        }
    };
    if animation_markers
        .iter()
        .any(|animation_marker| animation_marker.marker_name.is_empty())
    {
        return Err(ConversionError::InvalidSource(
            "KF contains an empty text key",
        ));
    }
    Ok(animation_markers)
}

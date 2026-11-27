use super::{
    conversion_error::ConversionError,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationMarker,
    animation_text_key::{AuthoredAnimationTextAction, AuthoredAnimationTextKey},
};

pub(super) struct KfTextKeys {
    pub(super) markers: Vec<AuthoredAnimationMarker>,
    pub(super) commands: Vec<AuthoredAnimationTextKey>,
    pub(super) end_time_seconds: f32,
}

pub(super) fn extract_netimmerse_kf_text_keys(
    netimmerse_kf_document: &NetImmerseNifDocument,
    text_key_block_reference: i32,
    sidecar: &[AuthoredAnimationTextKey],
) -> Result<KfTextKeys, ConversionError> {
    let keys = match netimmerse_kf_document
        .block(text_key_block_reference)
        .map(|block| &block.payload)
    {
        Some(NetImmerseNifBlockPayload::NiTextKeyExtraData(data)) => data.keys.as_slice(),
        None if text_key_block_reference < 0 => &[],
        _ => {
            return Err(ConversionError::InvalidSource(
                "KF has an invalid text-key reference",
            ));
        }
    };
    let mut markers = Vec::with_capacity(keys.len());
    let mut embedded_commands = Vec::new();
    let mut end_time_seconds = 0.0_f32;
    for key in keys {
        let text = key.value.trim_end_matches('\0').trim();
        if text.is_empty() {
            continue;
        }
        if !key.time.is_finite() {
            return Err(ConversionError::InvalidSource(
                "KF text key has a non-finite timestamp",
            ));
        }
        end_time_seconds = end_time_seconds.max(key.time);
        markers.push(AuthoredAnimationMarker {
            event_time_milliseconds: (key.time.max(0.0) * 1000.0).round() as u32,
            marker_name: text.to_owned(),
        });
        if sidecar.is_empty() {
            let marker = text.to_ascii_lowercase();
            if matches!(marker.as_str(), "loop_start" | "loop_end")
                || (!marker.contains(char::is_whitespace) && marker.ends_with("_sync"))
            {
                continue;
            }
            if let Some(mut key) =
                crate::source_document::animation_text_keys::parse_blue_fang_embedded_animation_text_key(key.time, text)
            {
                key.animation_text_commands.retain(|command| !matches!(
                    command.animation_text_action,
                    AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_)
                ));
                if !key.animation_text_commands.is_empty() {
                    embedded_commands.push(key);
                }
            }
        }
    }
    let commands = if sidecar.is_empty() {
        embedded_commands
    } else {
        sidecar.to_vec()
    };
    Ok(KfTextKeys {
        markers,
        commands,
        end_time_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model_source::native_animation_source_lowering::lower_native_animation_source,
        source_document::animation_text_keys::parse_blue_fang_embedded_animation_text_key,
    };

    fn write_string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn keyframe_source(sequence: &str) -> Vec<u8> {
        let mut bytes = b"NetImmerse File Format, Version 10.0.1.0\n".to_vec();
        bytes.extend_from_slice(&0x0a00_0100_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        write_string(&mut bytes, sequence);
        write_string(&mut bytes, "NiTextKeyExtraData");
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());

        bytes.extend_from_slice(&0_u32.to_le_bytes());
        write_string(&mut bytes, "sample");
        // The sequence-specific word is a zero policy or an empty root name.
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_i32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());

        bytes.extend_from_slice(&1_u32.to_le_bytes());
        write_string(&mut bytes, "Text Keys");
        let keys = [
            (0.0_f32, "start"),
            (0.25, ""),
            (0.375, "{Bip01 R Finger0} attach object, snd pickup"),
            (0.375, "right_foot_sync"),
            (1.25, "end"),
        ];
        bytes.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        for (seconds, text) in keys {
            bytes.extend_from_slice(&seconds.to_le_bytes());
            write_string(&mut bytes, text);
        }
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes
    }

    #[test]
    fn kf_commands_and_sync_markers_survive_sequence_loading() {
        for sequence in ["NiSequence", "NiControllerSequence"] {
            let source = keyframe_source(sequence);
            let mut clips = lower_native_animation_source("sample.kf", &source, &[]).unwrap();
            let (_, clip, metadata) = clips.pop().unwrap();
            assert_eq!(clip.duration(), 1.25);
            assert_eq!(metadata.animation_markers[2].marker_name, "right_foot_sync");
            assert_eq!(metadata.animation_markers[2].event_time_milliseconds, 375);
            let [key] = metadata.authored_animation_text_keys.as_slice() else {
                panic!("expected one command key");
            };
            assert_eq!(key.authored_time_seconds, Some(0.375));
            let [attach, sound] = key.animation_text_commands.as_slice() else {
                panic!("expected both commands in source order");
            };
            assert_eq!(
                attach.target_skeleton_joint_asset_key.as_deref(),
                Some("Bip01 R Finger0"),
            );
            assert!(matches!(
                attach.animation_text_action,
                AuthoredAnimationTextAction::AttachPendingObject
            ));
            assert!(matches!(
                sound.animation_text_action,
                AuthoredAnimationTextAction::PlaySound { .. }
            ));

            let sidecar =
                [parse_blue_fang_embedded_animation_text_key(0.5, "snd override").unwrap()];
            let mut clips = lower_native_animation_source("sample.kf", &source, &sidecar).unwrap();
            let (_, _, overridden) = clips.pop().unwrap();
            assert_eq!(overridden.authored_animation_text_keys, sidecar);
            assert!(overridden
                .animation_markers
                .iter()
                .map(|marker| { (marker.event_time_milliseconds, marker.marker_name.as_str()) })
                .eq(metadata.animation_markers.iter().map(|marker| {
                    (marker.event_time_milliseconds, marker.marker_name.as_str())
                })));
        }
    }
}

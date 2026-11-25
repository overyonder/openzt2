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

pub(super) fn extract_netimmerse_kf_text_keys(
    netimmerse_kf_document: &NetImmerseNifDocument,
    text_key_block_reference: i32,
    sidecar: &[AuthoredAnimationTextKey],
) -> Result<(Vec<AuthoredAnimationMarker>, Vec<AuthoredAnimationTextKey>), ConversionError> {
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
    for key in keys {
        let text = key.value.trim_end_matches('\0').trim();
        if text.is_empty() {
            return Err(ConversionError::InvalidSource(
                "KF contains an empty text key",
            ));
        }
        markers.push(AuthoredAnimationMarker {
            event_time_milliseconds: (key.time.max(0.0) * 1000.0).round() as u32,
            marker_name: text.to_owned(),
        });
        if sidecar.is_empty()
            && !["loop_start", "loop_end"]
                .iter()
                .any(|marker| text.eq_ignore_ascii_case(marker))
        {
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
    Ok((markers, commands))
}

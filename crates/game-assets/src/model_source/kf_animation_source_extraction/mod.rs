use super::{
    conversion_error::ConversionError,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};
use openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationMarker;

pub(super) fn extract_netimmerse_kf_animation_markers(
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

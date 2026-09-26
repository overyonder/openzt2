//! Blue Fang `.bf` animation conversion entry.

use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::{AuthoredAnimationTextAction, AuthoredAnimationTextKey},
};

use super::{
    bf_animation_glb_and_metadata_lowering::lower_parsed_blue_fang_animation_to_glb_and_authored_clip_metadata,
    bf_animation_source_parsing::parse_blue_fang_animation_source_bytes,
    conversion_error::ConversionError,
};

/// Converts one Blue Fang animation clip into an animation-only GLB
/// and its authored clip metadata.
pub(crate) fn convert_blue_fang_animation_source_to_glb_and_authored_clip_metadata(
    animation_clip_asset_path: &str,
    source_bytes: &[u8],
    authored_animation_text_keys: &[AuthoredAnimationTextKey],
) -> Result<(Vec<u8>, AuthoredAnimationClipMetadata), ConversionError> {
    if authored_animation_text_keys
        .iter()
        .flat_map(|authored_text_key| &authored_text_key.animation_text_commands)
        .any(|authored_text_command| {
            matches!(
                &authored_text_command.animation_text_action,
                AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_)
            )
        })
    {
        return Err(ConversionError::InvalidSource(
            "animation text keys contain an unlowered command",
        ));
    }
    let parsed_blue_fang_animation = parse_blue_fang_animation_source_bytes(source_bytes)?;
    lower_parsed_blue_fang_animation_to_glb_and_authored_clip_metadata(
        animation_clip_asset_path,
        parsed_blue_fang_animation,
        authored_animation_text_keys,
    )
}

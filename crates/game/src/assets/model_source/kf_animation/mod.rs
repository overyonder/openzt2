//! NetImmerse KF animation conversion entry.

use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::{AuthoredAnimationTextAction, AuthoredAnimationTextKey},
};

use super::{
    conversion_error::ConversionError,
    kf_animation_glb_and_metadata_lowering::lower_parsed_netimmerse_kf_animation_to_glb_and_authored_clip_metadata,
    kf_animation_source_extraction::extract_netimmerse_kf_animation_from_document,
    netimmerse_nif_source::document_source_types::NetImmerseNifDocument,
};

pub(crate) fn convert_netimmerse_kf_animation_to_glb_and_authored_clip_metadata(
    animation_clip_asset_path: &str,
    netimmerse_kf_document: &NetImmerseNifDocument,
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
    let parsed_netimmerse_kf_animation =
        extract_netimmerse_kf_animation_from_document(netimmerse_kf_document)?;
    lower_parsed_netimmerse_kf_animation_to_glb_and_authored_clip_metadata(
        animation_clip_asset_path,
        parsed_netimmerse_kf_animation,
        authored_animation_text_keys,
    )
}

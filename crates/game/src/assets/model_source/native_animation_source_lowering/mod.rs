//! Native BF/KF animation dispatch into GLB and authored clip metadata.

use std::path::Path;

use anyhow::Result;
use openzt2_game_data::animation::{
    animation_clip_metadata::AuthoredAnimationClipMetadata,
    animation_text_key::AuthoredAnimationTextKey,
};

use super::{
    animation::convert_blue_fang_animation_source_to_glb_and_authored_clip_metadata,
    conversion_error::ConversionError,
    kf_animation::convert_netimmerse_kf_animation_to_glb_and_authored_clip_metadata,
    netimmerse_nif_source::document_source_types::NetImmerseNifDocument,
};

pub(in crate::assets) fn lower_native_animation_source(
    animation_clip_asset_path: &str,
    animation_source_bytes: &[u8],
    authored_animation_text_keys: &[AuthoredAnimationTextKey],
) -> Result<(Vec<u8>, AuthoredAnimationClipMetadata)> {
    match Path::new(animation_clip_asset_path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("bf") => {
            convert_blue_fang_animation_source_to_glb_and_authored_clip_metadata(
                animation_clip_asset_path,
                animation_source_bytes,
                authored_animation_text_keys,
            )
            .map_err(Into::into)
        }
        Some(extension) if extension.eq_ignore_ascii_case("kf") => {
            let netimmerse_document = NetImmerseNifDocument::parse(
                animation_clip_asset_path.to_owned(),
                animation_source_bytes,
            )?;
            convert_netimmerse_kf_animation_to_glb_and_authored_clip_metadata(
                animation_clip_asset_path,
                &netimmerse_document,
                authored_animation_text_keys,
            )
            .map_err(Into::into)
        }
        _ => Err(ConversionError::InvalidSource("unsupported animation source").into()),
    }
}

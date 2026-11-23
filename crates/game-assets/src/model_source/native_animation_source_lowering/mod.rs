use std::path::Path;

use bevy::{
    animation::{animated_field, AnimationTargetId},
    prelude::*,
};
use openzt2_game_data::animation::{
    animation_clip_metadata::{
        AuthoredAnimationClipMetadata, AuthoredAnimationRootMotionJoint,
        AuthoredAnimationRootMotionTransform,
    },
    animation_text_key::{AuthoredAnimationTextAction, AuthoredAnimationTextKey},
};

use super::{
    animation::lower_blue_fang_clip, conversion_error::ConversionError,
    kf_animation::lower_netimmerse_kf_clip,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};

pub(crate) fn kf_sequence_label(sequence_index: usize) -> String {
    format!("Sequence/{sequence_index}")
}

#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
pub(crate) fn lower_native_animation_source(
    path: &str,
    bytes: &[u8],
    text_keys: &[AuthoredAnimationTextKey],
) -> anyhow::Result<Vec<(usize, AnimationClip, AuthoredAnimationClipMetadata)>> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str());
    let clips = match extension {
        Some(extension) if extension.eq_ignore_ascii_case("bf") => {
            let (clip, metadata) = lower_blue_fang_clip(path, bytes, text_keys)?;
            vec![(0, clip, metadata)]
        }
        Some(extension) if extension.eq_ignore_ascii_case("kf") => {
            let document = NetImmerseNifDocument::parse(path.to_owned(), bytes)?;
            let mut clips = Vec::new();
            for (index, &reference) in document.footer.root_block_references.iter().enumerate() {
                if let Some(NetImmerseNifBlockPayload::NiControllerSequence(sequence)) =
                    document.block(reference).map(|block| &block.payload)
                {
                    let (clip, metadata) =
                        lower_netimmerse_kf_clip(path, &document, sequence, text_keys)?;
                    clips.push((index, clip, metadata));
                }
            }
            if clips.is_empty() {
                return Err(
                    ConversionError::InvalidSource("KF has no controller sequence root").into(),
                );
            }
            clips
        }
        _ => return Err(ConversionError::InvalidSource("unsupported animation source").into()),
    };
    if clips
        .iter()
        .flat_map(|(_, _, metadata)| &metadata.authored_animation_text_keys)
        .flat_map(|key| &key.animation_text_commands)
        .any(|command| {
            matches!(
                &command.animation_text_action,
                AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_)
            )
        })
    {
        return Err(ConversionError::InvalidSource(
            "animation text keys contain an unlowered command",
        )
        .into());
    }
    Ok(clips)
}

pub(super) fn sample_root_motion(
    clip: &AnimationClip,
    joint_name: &str,
) -> (
    AuthoredAnimationRootMotionJoint,
    AuthoredAnimationRootMotionTransform,
) {
    let lower_name = joint_name.to_ascii_lowercase();
    let target = AnimationTargetId::from_iter([lower_name.as_str()]);
    let translation_at = |time| {
        clip.sample_clamped(animated_field!(Transform::translation), target, time)
            .unwrap_or(Vec3::ZERO)
    };
    let rotation_at = |time| {
        clip.sample_clamped(animated_field!(Transform::rotation), target, time)
            .unwrap_or(Quat::IDENTITY)
    };
    let initial = translation_at(0.0);
    (
        AuthoredAnimationRootMotionJoint {
            joint_asset_key: joint_name.to_owned(),
            initial_translation: initial.to_array(),
        },
        AuthoredAnimationRootMotionTransform {
            translation_from_first_to_last_frame: (translation_at(clip.duration()) - initial)
                .to_array(),
            rotation_from_first_to_last_frame: (rotation_at(clip.duration())
                * rotation_at(0.0).inverse())
            .to_array(),
        },
    )
}

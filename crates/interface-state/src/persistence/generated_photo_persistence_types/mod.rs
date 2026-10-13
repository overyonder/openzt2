//! Durable generated-photo storage components and request-result messages.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone)]
pub struct DeleteGeneratedPhotoImageAndChallengeCopies {
    pub photo_entity: Entity,
    pub generated_image_handle: Handle<Image>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratedPhotoImageAndChallengeCopiesDeleted {
    pub photo_entity: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratedPhotoImageAndChallengeCopiesDeletionFailed {
    pub photo_entity: Entity,
}

/// Challenges with a saved JPEG copy of this photo.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct DurableChallengePhotoFileIdentifiers {
    pub challenge_definition_identifiers: Vec<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistChallengePhotoJpegCopy {
    pub photo_entity: Entity,
    pub challenge_definition_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengePhotoJpegCopyPersisted {
    pub photo_entity: Entity,
    pub challenge_definition_identifier: AssetId,
    pub encoded_jpeg_byte_count: u64,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengePhotoJpegCopyPersistenceFailed {
    pub photo_entity: Entity,
    pub challenge_definition_identifier: AssetId,
}

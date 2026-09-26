use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Prefab-local authored socket identity used by ordinary child attachment.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefabAuthoredAttachmentIdentifier(pub(crate) AssetId);

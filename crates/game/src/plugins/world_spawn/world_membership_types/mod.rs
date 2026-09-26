//! World root, membership, and authored-definition identities.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldRoot {
    pub(crate) scenario: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldMember {
    pub(crate) root: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefinitionId(pub(crate) AssetId);

//! World root, membership, and authored-definition identities.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldRoot {
    pub scenario: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = WorldMembers)]
pub struct WorldMember {
    pub root: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = WorldMember, linked_spawn)]
pub struct WorldMembers(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionId(pub AssetId);

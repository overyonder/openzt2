use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FencePreview {
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathPreview {
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlaceFence {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacePath {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

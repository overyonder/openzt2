use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::FencePlacementShape;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FencePreview {
    pub definition: AssetId,
    pub from: IVec3,
    pub to: IVec3,
    /// A closed shape spans `from` and `to` as opposite corners.
    pub shape: FencePlacementShape,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathPreview {
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceFence {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacePath {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) from: IVec3,
    pub(crate) to: IVec3,
}

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
    pub heading_eighth_turns: u8,
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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct FenceDrawPlan {
    pub input: FencePreview,
    pub version: TopologyPlanVersion,
    pub quote: Option<(
        openzt2_zoo_state::economy::money_types::Money,
        Option<openzt2_zoo_state::topology::gate_operation_types::Gate>,
        Vec<Entity>,
    )>,
    pub cells: Vec<IVec3>,
    pub replaced: Option<Entity>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct PathDrawPlan {
    pub input: PathPreview,
    pub version: TopologyPlanVersion,
    pub approved: Vec<IVec3>,
    pub cells: Vec<IVec3>,
    pub validity:
        openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TopologyPlanVersion {
    pub revisions: [bevy::ecs::change_detection::Tick; 6],
    pub terrain_changes: usize,
}

pub(super) fn terrain_change_cursor(
    changes: &Messages<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>,
) -> usize {
    changes.oldest_message_count() + changes.len()
}

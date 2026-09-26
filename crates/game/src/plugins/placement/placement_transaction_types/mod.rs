use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::{
    assets::scene_prefab::ScenePrefabAsset,
    plugins::{
        construction::{
            construction_interaction_types::PlacementFailure,
            construction_transaction_types::EditApplication,
        },
        economy::money_types::Money,
        world_spawn::persistent_id_types::PersistentId,
    },
};

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct EvaluateObjectPlacementPreviewRequest {
    pub preview: Entity,
    pub definition: AssetId,
    pub transform: Transform,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitObjectPlacementPreviewRequest {
    pub preview: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovePlacedObjectRequest {
    pub entity: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementCommitted {
    pub transaction: Entity,
    pub entity: Entity,
    pub definition: AssetId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedObjectPlacementEditMutationKind {
    Create,
    Remove,
    Relocate,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct PreparedObjectPlacementEdit {
    pub mutations: Box<[PreparedObjectPlacementMutation]>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedObjectPlacementMutation {
    pub entity: PersistentId,
    pub definition: AssetId,
    pub transform: Transform,
    pub cells: Box<[IVec2]>,
    pub previous_transform: Option<Transform>,
    pub previous_cells: Box<[IVec2]>,
    pub mutation: PreparedObjectPlacementEditMutationKind,
    pub applied_entity: Option<Entity>,
    pub preview: Option<Entity>,
    pub prefab: Handle<ScenePrefabAsset>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedObjectRelocationSource(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectPlacementEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectPlacementEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplyPreparedObjectPlacementEditRequest {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectPlacementEditApplicationAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedObjectTerrainSupportOutcome {
    Stable,
    Relocated,
    Rejected(PlacementFailure),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedObjectTerrainSupportEvaluated {
    pub entity: Entity,
    pub outcome: PlacedObjectTerrainSupportOutcome,
}

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_transaction_types::EditApplication;
use crate::economy::money_types::Money;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct EvaluateObjectPlacementPreviewRequest {
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
pub enum PreparedObjectPlacementEditMutationKind {
    Create,
    Remove,
    Relocate,
}

#[derive(Component, Debug, Clone)]
pub struct PreparedObjectPlacementEdit {
    pub mutations: Box<[PreparedObjectPlacementMutation]>,
}

#[derive(Debug, Clone)]
pub struct PreparedObjectPlacementMutation {
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
pub struct PlacedObjectRelocationSource(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementEditPrepared {
    pub transaction: Entity,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementEditPreparationRejected {
    pub transaction: Entity,
    pub reason: PlacementFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyPreparedObjectPlacementEditRequest {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementEditApplicationAcknowledged {
    pub transaction: Entity,
    pub application: EditApplication,
    pub accepted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacedObjectTerrainSupportOutcome {
    Stable,
    Relocated,
    Rejected(PlacementFailure),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedObjectTerrainSupportEvaluated {
    pub entity: Entity,
    pub outcome: PlacedObjectTerrainSupportOutcome,
}

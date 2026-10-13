use bevy::prelude::*;

pub mod construction_edit_history_types;
pub mod construction_interaction_types;
pub mod construction_tool_and_placement_policy_types;
pub mod construction_transaction_types;
pub mod selected_biome_ui_projection;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedConstructionSet {
    Prepare,
    RequestEconomy,
    SettleEconomy,
    CompleteEconomy,
    Apply,
    Cleanup,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedConstructionTransactionMaterializationSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedConstructionPreparedDomainEditMaterializationSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionCursorUpdateSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionPreviewCancellationSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionDomainPreparationCollectionSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionDomainApplicationDispatchSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionFailureRollbackDispatchSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstructionDomainAcknowledgementCollectionSet;

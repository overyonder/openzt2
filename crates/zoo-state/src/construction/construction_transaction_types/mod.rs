use bevy::prelude::*;

use crate::economy::account_transaction_types::TransactionRejection;
use crate::economy::money_types::Money;

use crate::construction::construction_interaction_types::PlacementFailure;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditTransaction {
    pub sequence: u64,
    pub state: TransactionState,
    pub cost: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Applied,
    Undone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditApplication {
    InitialCommit,
    Undo,
    Redo,
    FailureRollback,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrepareConstruction {
    pub transaction: Entity,
    pub preview: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrepareDeletion {
    pub transaction: Entity,
    pub target: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditApplicationAuthorized {
    pub transaction: Entity,
    pub application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionCommitFailed {
    pub transaction: Entity,
    pub reason: ConstructionCommitFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct ConstructionCommitted {
    pub transaction: Entity,
    pub cost: Money,
    pub screen_position: Vec2,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ConstructionFeedbackOrigin(pub Vec2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionCommitFailure {
    NoDomain,
    Preparation(PlacementFailure),
    Economy(TransactionRejection),
    DomainRejected,
    Compensation(TransactionRejection),
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackContext {
    pub failed_application: EditApplication,
    pub failure: ConstructionCommitFailure,
}

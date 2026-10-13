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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainAckStatus {
    NotExpected,
    Pending,
    Applied,
    Reverted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCommitPhase {
    Preparing,
    AwaitingEconomy,
    Applying,
    RollingBack,
    AwaitingCompensation,
    Complete,
    Failed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditCommitProgress {
    pub phase: EditCommitPhase,
    pub application: EditApplication,
    pub cost: Money,
    pub terrain: DomainAckStatus,
    pub topology: DomainAckStatus,
    pub placement: DomainAckStatus,
    pub shows: DomainAckStatus,
    pub aquatic: DomainAckStatus,
}

impl EditCommitProgress {
    pub fn preparing() -> Self {
        Self {
            phase: EditCommitPhase::Preparing,
            application: EditApplication::InitialCommit,
            cost: Money(0),
            terrain: DomainAckStatus::NotExpected,
            topology: DomainAckStatus::NotExpected,
            placement: DomainAckStatus::NotExpected,
            shows: DomainAckStatus::NotExpected,
            aquatic: DomainAckStatus::NotExpected,
        }
    }

    pub fn expects_domain(&self) -> bool {
        self.terrain != DomainAckStatus::NotExpected
            || self.topology != DomainAckStatus::NotExpected
            || self.placement != DomainAckStatus::NotExpected
            || self.shows != DomainAckStatus::NotExpected
            || self.aquatic != DomainAckStatus::NotExpected
    }

    pub fn all_expected_are(&self, wanted: DomainAckStatus) -> bool {
        [
            self.terrain,
            self.topology,
            self.placement,
            self.shows,
            self.aquatic,
        ]
        .into_iter()
        .all(|status| status == DomainAckStatus::NotExpected || status == wanted)
    }

    pub fn reset_expected_to_pending(&mut self) {
        for status in [
            &mut self.terrain,
            &mut self.topology,
            &mut self.placement,
            &mut self.shows,
            &mut self.aquatic,
        ] {
            if *status != DomainAckStatus::NotExpected {
                *status = DomainAckStatus::Pending;
            }
        }
    }
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompensationRequested;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackDispatched;

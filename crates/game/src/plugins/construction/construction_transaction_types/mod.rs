use bevy::prelude::*;

use crate::plugins::economy::{
    account_transaction_types::TransactionRejection, money_types::Money,
};

use super::construction_interaction_types::PlacementFailure;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditTransaction {
    pub(crate) sequence: u64,
    pub(crate) state: TransactionState,
    pub(crate) cost: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionState {
    Applied,
    Undone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditApplication {
    InitialCommit,
    Undo,
    Redo,
    FailureRollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DomainAckStatus {
    NotExpected,
    Pending,
    Applied,
    Reverted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditCommitPhase {
    Preparing,
    AwaitingEconomy,
    Applying,
    RollingBack,
    AwaitingCompensation,
    Complete,
    Failed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditCommitProgress {
    pub(crate) phase: EditCommitPhase,
    pub(crate) application: EditApplication,
    pub(crate) cost: Money,
    pub(crate) terrain: DomainAckStatus,
    pub(crate) topology: DomainAckStatus,
    pub(crate) placement: DomainAckStatus,
    pub(crate) shows: DomainAckStatus,
}

impl EditCommitProgress {
    pub(crate) fn preparing() -> Self {
        Self {
            phase: EditCommitPhase::Preparing,
            application: EditApplication::InitialCommit,
            cost: Money(0),
            terrain: DomainAckStatus::NotExpected,
            topology: DomainAckStatus::NotExpected,
            placement: DomainAckStatus::NotExpected,
            shows: DomainAckStatus::NotExpected,
        }
    }

    pub(crate) fn expects_domain(&self) -> bool {
        self.terrain != DomainAckStatus::NotExpected
            || self.topology != DomainAckStatus::NotExpected
            || self.placement != DomainAckStatus::NotExpected
            || self.shows != DomainAckStatus::NotExpected
    }

    pub(crate) fn all_expected_are(&self, wanted: DomainAckStatus) -> bool {
        [self.terrain, self.topology, self.placement, self.shows]
            .into_iter()
            .all(|status| status == DomainAckStatus::NotExpected || status == wanted)
    }

    pub(crate) fn reset_expected_to_pending(&mut self) {
        for status in [
            &mut self.terrain,
            &mut self.topology,
            &mut self.placement,
            &mut self.shows,
        ] {
            if *status != DomainAckStatus::NotExpected {
                *status = DomainAckStatus::Pending;
            }
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrepareConstruction {
    pub(crate) transaction: Entity,
    pub(crate) preview: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrepareDeletion {
    pub(crate) transaction: Entity,
    pub(crate) target: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditApplicationAuthorized {
    pub(crate) transaction: Entity,
    pub(crate) application: EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConstructionCommitFailed {
    pub(crate) transaction: Entity,
    pub(crate) reason: ConstructionCommitFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConstructionCommitted {
    pub(crate) transaction: Entity,
    pub(crate) cost: Money,
    pub(crate) screen_position: Vec2,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConstructionFeedbackOrigin(pub(crate) Vec2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstructionCommitFailure {
    NoDomain,
    Preparation(PlacementFailure),
    Economy(TransactionRejection),
    DomainRejected,
    Compensation(TransactionRejection),
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RollbackContext {
    pub(super) failed_application: EditApplication,
    pub(super) failure: ConstructionCommitFailure,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompensationRequested;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RollbackDispatched;

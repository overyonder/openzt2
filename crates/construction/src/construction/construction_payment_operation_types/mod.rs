use bevy::prelude::*;

/// Correlates one economy operation with the construction transaction whose
/// transfer result it must advance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConstructionPaymentOperation {
    pub(super) construction_transaction: Entity,
    pub(super) purpose: ConstructionPaymentPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConstructionPaymentPurpose {
    InitialTransfer,
    RedoTransfer,
    FailureCompensation,
    UndoCompensation,
}

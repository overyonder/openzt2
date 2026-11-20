use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConstructionPaymentPurpose {
    InitialTransfer,
    RedoTransfer,
    FailureCompensation,
    UndoCompensation,
}

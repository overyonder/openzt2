use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAppliedConstructionTransaction(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyConstructionTransaction {
    pub transaction: Entity,
    pub direction: ConstructionEditDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionEditDirection {
    Undo,
    Redo,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionTransactionApplied {
    pub transaction: Entity,
    pub direction: ConstructionEditDirection,
    pub accepted: bool,
}

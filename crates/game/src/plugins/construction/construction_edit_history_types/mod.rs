use bevy::prelude::*;

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConstructionEditHistory {
    pub(crate) entries: Vec<Entity>,
    pub(crate) cursor: usize,
    pub(crate) limit: usize,
    pub(crate) next_sequence: u64,
}

impl Default for ConstructionEditHistory {
    fn default() -> Self {
        Self::with_limit(128)
    }
}

impl ConstructionEditHistory {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            entries: Vec::with_capacity(limit),
            cursor: 0,
            limit,
            next_sequence: 1,
        }
    }

    pub(crate) fn allocate_sequence(&mut self) -> Option<u64> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1)?;
        Some(sequence)
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecordAppliedConstructionTransaction(pub(crate) Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplyConstructionTransaction {
    pub(crate) transaction: Entity,
    pub(crate) direction: ConstructionEditDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstructionEditDirection {
    Undo,
    Redo,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConstructionTransactionApplied {
    pub(crate) transaction: Entity,
    pub(crate) direction: ConstructionEditDirection,
    pub(crate) accepted: bool,
}

use bevy::prelude::*;

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct ConstructionEditHistory {
    pub entries: Vec<Entity>,
    pub cursor: usize,
    pub limit: usize,
    pub next_sequence: u64,
}

impl Default for ConstructionEditHistory {
    fn default() -> Self {
        Self::with_limit(128)
    }
}

impl ConstructionEditHistory {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            entries: Vec::with_capacity(limit),
            cursor: 0,
            limit,
            next_sequence: 1,
        }
    }

    pub fn allocate_sequence(&mut self) -> Option<u64> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1)?;
        Some(sequence)
    }
}

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

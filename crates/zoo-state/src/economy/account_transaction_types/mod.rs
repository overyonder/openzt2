use bevy::prelude::*;

use super::money_types::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Admission,
    Purchase,
    AnimalAdoption,
    Research,
    StaffHire,
    Service,
    Construction,
    Wage,
    Donation,
    Reward,
    Refund,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Account {
    Zoo,
    Entity(Entity),
    External,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionRequest {
    pub operation: Entity,
    pub debit: Account,
    pub credit: Account,
    pub amount: Money,
    pub kind: TransactionKind,
    pub subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionCompleted {
    pub operation: Entity,
    pub debit: Account,
    pub credit: Account,
    pub amount: Money,
    pub kind: TransactionKind,
    pub subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionRejected {
    pub operation: Entity,
    pub debit: Account,
    pub credit: Account,
    pub amount: Money,
    pub kind: TransactionKind,
    pub subject: Option<Entity>,
    pub reason: TransactionRejection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionRejection {
    Cancelled,
    NonPositive,
    InsufficientFunds,
    MissingAccount,
    Overflow,
}

/// Settlement belongs to the operation that requested the transfer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionSettlement {
    Pending {
        request: TransactionRequest,
        sequence: usize,
        player_operation: bool,
    },
    Settled {
        player_operation: bool,
    },
}

impl TransactionSettlement {
    pub const fn is_settled_in_domain(&self, player: bool) -> bool {
        matches!(self, Self::Settled { player_operation } if *player_operation == player)
    }
}

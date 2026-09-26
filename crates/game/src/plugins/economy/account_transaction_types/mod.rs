use bevy::prelude::*;

use super::money_types::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionKind {
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
pub(crate) enum Account {
    Zoo,
    Entity(Entity),
    External,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransactionRequest {
    pub(crate) operation: Entity,
    pub(crate) debit: Account,
    pub(crate) credit: Account,
    pub(crate) amount: Money,
    pub(crate) kind: TransactionKind,
    pub(crate) subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransactionCompleted {
    pub(crate) operation: Entity,
    pub(crate) debit: Account,
    pub(crate) credit: Account,
    pub(crate) amount: Money,
    pub(crate) kind: TransactionKind,
    pub(crate) subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransactionRejected {
    pub(crate) operation: Entity,
    pub(crate) debit: Account,
    pub(crate) credit: Account,
    pub(crate) amount: Money,
    pub(crate) kind: TransactionKind,
    pub(crate) subject: Option<Entity>,
    pub(crate) reason: TransactionRejection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionRejection {
    Cancelled,
    NonPositive,
    InsufficientFunds,
    MissingAccount,
    Overflow,
}

/// Suppresses replay of a caller-owned operation after first settlement.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransactionSettled;

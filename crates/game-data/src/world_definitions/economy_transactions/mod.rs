//! Named economic operations retained by the authored object definition.

use serde::{Deserialize, Serialize};

use crate::AssetId;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum EconomyTransactionKind {
    Debit,
    Credit,
    SetCash,
    AddUser,
    RemoveUser,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum EconomyTransactionCostBasis {
    Fixed,
    Parent,
    PercentParent,
    Random,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum EconomyTransactionPeriod {
    Demand,
    Once,
    Daily,
    Monthly,
    Yearly,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EconomyTransactionDefinition {
    pub name: AssetId,
    pub category: AssetId,
    pub kind: EconomyTransactionKind,
    pub cost_basis: EconomyTransactionCostBasis,
    /// Authored scalar: currency units for money, entity count for user operations.
    pub cost: f32,
    pub minimum_cost: f32,
    pub maximum_cost: f32,
    pub cost_step: f32,
    pub cost_choices: Vec<f32>,
    pub initial_cost_index: usize,
    pub period: EconomyTransactionPeriod,
    pub frequency: i32,
    pub aggregate: bool,
    pub track_on_parent: bool,
    pub target: Option<AssetId>,
    pub next_transaction: Option<AssetId>,
}

use bevy::prelude::*;

use super::money_types::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioEconomyOperation {
    GrantCash(Money),
    TakeCash(Money),
    SetAdmission(Money),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioEconomyCommand {
    pub terminal: Entity,
    pub operation: ScenarioEconomyOperation,
}

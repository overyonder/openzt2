use bevy::prelude::*;

use super::money_types::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScenarioEconomyOperation {
    GrantCash(Money),
    TakeCash(Money),
    SetAdmission(Money),
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioEconomyCommand {
    pub(crate) terminal: Entity,
    pub(crate) operation: ScenarioEconomyOperation,
}

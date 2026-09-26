//! Transportation circuit, vehicle generation, and path-deletion actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiTransportActionRecord {
    pub trigger: UiTrigger,
    pub action: UiTransportAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiTransportAction {
    SetSelectedTransportCircuitOpen { open: bool },
    ReverseSelectedTransportCircuitDirection,
    GenerateVehicleForSelectedTransportCircuit,
    ResolvePendingTransportPathDeletion { deletion_confirmed: bool },
}

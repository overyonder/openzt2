//! Simulation pause actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiSimulationActionRecord {
    pub trigger: UiTrigger,
    pub action: UiSimulationAction,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSimulationAction {
    SetSimulationPaused { paused: bool },
    ToggleSimulationPaused,
}

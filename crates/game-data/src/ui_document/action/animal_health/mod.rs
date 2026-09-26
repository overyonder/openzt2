//! Animal disease-treatment and tranquilizer tool actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiAnimalHealthActionRecord {
    pub trigger: UiTrigger,
    pub action: UiAnimalHealthAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiAnimalHealthAction {
    EnterDiseaseTreatmentMode,
    EnterTranquilizerMode,
}

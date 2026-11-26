//! Staff assignment, worker-duty, and firing actions.

use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiStaffActionRecord {
    pub trigger: UiTrigger,
    pub action: UiStaffAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiStaffAction {
    SetKeeperAssignmentSelection { assignment_index: i32 },
    SetTrainerAssignmentSelection { assignment_index: i32 },
    FireSelectedStaff,
    MarkSelectedEntityAsKeeperAssignmentTarget,
    ClearSelectedKeeperAssignmentTarget,
    SetWorkerDutyAssignment { task: AssetId, assigned: bool },
    EnterStaffAssignmentMode,
}

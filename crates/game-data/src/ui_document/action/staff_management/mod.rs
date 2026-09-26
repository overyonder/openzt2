//! Staff assignment, worker-duty, and firing actions.

use super::UiTrigger;
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
    SetWorkerDutyAssignment { duty: UiWorkerDuty, assigned: bool },
    EnterStaffAssignmentMode,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiWorkerDuty {
    CleanAquaticTankFilter,
    EmptyRecyclingContainer,
    EmptyTrashContainer,
    SweepLitter,
}

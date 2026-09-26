//! Immersive activity mode entry actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiEnterImmersiveModeActionRecord {
    pub trigger: UiTrigger,
    pub mode: UiImmersiveModeKind,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiImmersiveModeKind {
    SelectedEntityFirstPerson,
    Cloning,
    FossilSearch,
    FossilAssembly,
    GuestView,
    Photo,
    /// Walk through the zoo in first person as the super-staff avatar, the
    /// default child of the first-person mode.
    SuperStaff,
}

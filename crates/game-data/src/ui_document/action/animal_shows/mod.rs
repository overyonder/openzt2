//! Animal-show editing, scheduling, mixer, and platform-upgrade actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiShowActionRecord {
    pub trigger: UiTrigger,
    pub action: UiShowAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiShowAction {
    ToggleSelectedEnabled,
    EditSelected,
    CancelEdit,
    SetSelectedName,
    ShowMixerPanel,
    ToggleEditing,
    DismissMixerDropdown,
    SetSelectedOpen { open: bool },
    RequestAddShow,
    AddShow,
    RequestDeleteSelected,
    DeleteSelected,
    ViewSelected,
    AddBreak,
    MoveSelectedUp,
    MoveSelectedDown,
    PurchaseSelectedPlatformUpgrade,
    SelectPlatformUpgrade { upgrade: UiShowPlatformUpgrade },
    ResolvePlatformDeletion { confirmed: bool },
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiShowPlatformUpgrade {
    Television,
    Canopy,
}

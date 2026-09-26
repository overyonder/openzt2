//! Save, load, profile, and persistence-dialog actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiPersistenceActionRecord {
    pub trigger: UiTrigger,
    pub action: UiPersistenceAction,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPersistenceAction {
    SaveWorldSnapshotToSlot { save_slot: u32 },
    LoadWorldSnapshotFromSlot { save_slot: u32 },
    SaveWorldSnapshotToSelectedSlot,
    DeleteWorldSnapshotFromSelectedSlot,
    CreateProfileFromSubmittedDisplayName,
    SelectProfileAtActivatedRow,
    DeleteProfileAtActivatedRow,
    LoadWorldSnapshotFromSelectedSlot,
    OpenSaveSlotCatalogueForSaving,
    OpenSaveSlotCatalogueForLoading,
    OpenLoadSlotCatalogueAfterSaveCompletes,
}

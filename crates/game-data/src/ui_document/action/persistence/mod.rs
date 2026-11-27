//! Save, load, profile, and persistence-dialog actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiPersistenceActionRecord {
    pub trigger: UiTrigger,
    pub action: UiPersistenceAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiPersistenceAction {
    SetProfileAttribute {
        key: String,
        value: String,
    },
    SaveWorldSnapshotToSlot {
        save_slot: u32,
    },
    LoadWorldSnapshotFromSlot {
        save_slot: u32,
    },
    /// Saves under the name typed in the dialog. A name an existing save
    /// already has asks first, unless the overwrite has been confirmed.
    SaveWorldSnapshotUnderSubmittedName {
        overwrite_confirmed: bool,
    },
    DeleteWorldSnapshotFromSelectedSlot,
    CreateProfileFromSubmittedDisplayName,
    SelectProfileAtActivatedRow,
    DeleteProfileAtActivatedRow,
    LoadWorldSnapshotFromSelectedSlot,
    OpenSaveSlotCatalogueForSaving,
    OpenSaveSlotCatalogueForLoading,
    OpenLoadSlotCatalogueAfterSaveCompletes,
}

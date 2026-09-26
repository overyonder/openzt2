//! Persistence UI presentation state and action-message writers.

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiRole;

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailed,
    profile_types::{CreateProfile, DeleteProfile, SelectProfile},
    save_slot_types::{
        DeleteWorldSnapshotFromSlot, LoadSaveSlotCatalogue, LoadWorldSnapshotFromSlot,
        SaveWorldSnapshotToSlot,
    },
};

/// Marks an owner whose saved-games document is presenting save slots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SaveSlotCataloguePresentedForSaving;

/// Marks an owner whose saved-games document is presenting load slots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SaveSlotCataloguePresentedForLoading;

/// Open the load-slot menu after the save succeeds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OpenLoadSlotCatalogueAfterWorldSnapshotSave;

#[derive(SystemParam)]
pub(super) struct PersistenceUiActionMessageWriters<'w> {
    pub(super) save_world_snapshot_requests: MessageWriter<'w, SaveWorldSnapshotToSlot>,
    pub(super) load_world_snapshot_requests: MessageWriter<'w, LoadWorldSnapshotFromSlot>,
    pub(super) delete_world_snapshot_requests: MessageWriter<'w, DeleteWorldSnapshotFromSlot>,
    pub(super) create_profile_requests: MessageWriter<'w, CreateProfile>,
    pub(super) select_profile_requests: MessageWriter<'w, SelectProfile>,
    pub(super) delete_profile_requests: MessageWriter<'w, DeleteProfile>,
    pub(super) show_ui_document_requests: MessageWriter<'w, ShowUiRole>,
    pub(super) load_save_slot_catalogue_requests: MessageWriter<'w, LoadSaveSlotCatalogue>,
    pub(super) world_snapshot_persistence_failures:
        MessageWriter<'w, WorldSnapshotPersistenceFailed>,
}

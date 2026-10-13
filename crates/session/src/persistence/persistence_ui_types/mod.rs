//! Persistence UI presentation state and action-message writers.

use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;

use openzt2_interface_state::persistence::profile_types::CreateProfile;
use openzt2_interface_state::persistence::profile_types::DeleteProfile;
use openzt2_interface_state::persistence::profile_types::SelectProfile;
use openzt2_interface_state::persistence::save_slot_types::DeleteWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::LoadSaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::LoadWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::SaveWorldSnapshotToSlot;

use super::persistence_failure_types::WorldSnapshotPersistenceFailed;

/// Open the load-slot menu after the save succeeds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenLoadSlotCatalogueAfterWorldSnapshotSave;

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

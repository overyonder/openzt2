use bevy::{asset::UntypedAssetId, prelude::*};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

/// Marker for the root of the main-menu mod controls.
#[derive(Component)]
pub(super) struct ModManagerOverlayRoot;

/// Full-canvas input owner shown while one archive change is settling.
#[derive(Component)]
pub(super) struct ModManagerReloadInputBlocker;

/// Index into the asset source's priority-ordered archive status collection.
#[derive(Component)]
pub(super) struct AssetArchiveEnabledStateCheckbox {
    pub(super) archive_status_index: usize,
}

/// Loaded Bevy assets which must settle before another archive mutation or
/// shell interaction is accepted.
#[derive(Resource, Default)]
pub(crate) struct AssetArchiveReloadState {
    pending_loaded_asset_identifiers: Vec<UntypedAssetId>,
    main_menu_document: Option<Handle<UiDocumentAsset>>,
    asset_source_event_observation_updates_remaining: u8,
}

impl AssetArchiveReloadState {
    pub(crate) fn is_in_progress(&self) -> bool {
        self.main_menu_document.is_some()
    }

    pub(super) fn begin(
        &mut self,
        pending_loaded_asset_identifiers: Vec<UntypedAssetId>,
        main_menu_document: Handle<UiDocumentAsset>,
    ) {
        self.pending_loaded_asset_identifiers = pending_loaded_asset_identifiers;
        self.main_menu_document = Some(main_menu_document);
        self.asset_source_event_observation_updates_remaining = 2;
    }

    pub(super) fn wait_for_asset_source_event_observation(&mut self) -> bool {
        if self.asset_source_event_observation_updates_remaining == 0 {
            return false;
        }
        self.asset_source_event_observation_updates_remaining -= 1;
        true
    }

    pub(super) fn pending_loaded_asset_identifiers(&self) -> &[UntypedAssetId] {
        &self.pending_loaded_asset_identifiers
    }

    pub(super) fn main_menu_document(&self) -> Option<&Handle<UiDocumentAsset>> {
        self.main_menu_document.as_ref()
    }

    pub(super) fn finish(&mut self) {
        self.pending_loaded_asset_identifiers.clear();
        self.main_menu_document = None;
        self.asset_source_event_observation_updates_remaining = 0;
    }
}

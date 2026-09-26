use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    blue_fang_localization_locale_recognition::source_path_contains_english_localization,
    loaded_localization_queries::LoadedLocalizationView,
    localization_asset_types::LocalizationAsset,
};

#[derive(Resource, Default)]
pub(crate) struct LocalizationPrecedenceIndex {
    indexed_archive_revision: Option<u64>,
    localization_index_is_dirty: bool,
    localization_asset_handles: Vec<Handle<LocalizationAsset>>,
    localization_entry_asset_index: BTreeMap<AssetId, Handle<LocalizationAsset>>,
}

impl LocalizationPrecedenceIndex {
    pub(crate) fn borrow_loaded_localization_view<'a>(
        &'a self,
        localization_assets: &'a Assets<LocalizationAsset>,
    ) -> Option<LoadedLocalizationView<'a>> {
        (!self.localization_index_is_dirty && !self.localization_asset_handles.is_empty())
            .then_some(LoadedLocalizationView::new(self, localization_assets))
    }

    pub(super) fn find_asset_for_localization_entry<'a>(
        &'a self,
        localization_assets: &'a Assets<LocalizationAsset>,
        localization_entry_identifier: AssetId,
    ) -> Option<&'a LocalizationAsset> {
        localization_assets.get(
            self.localization_entry_asset_index
                .get(&localization_entry_identifier)?,
        )
    }

    pub(super) fn localization_asset_handles(&self) -> &[Handle<LocalizationAsset>] {
        &self.localization_asset_handles
    }
}

pub(super) fn load_english_localization_assets_after_archive_revision(
    archives: Res<crate::asset_source::AssetArchives>,
    asset_server: Res<AssetServer>,
    mut localization_precedence_index: ResMut<LocalizationPrecedenceIndex>,
) {
    let archive_revision = archives.revision();
    if localization_precedence_index.indexed_archive_revision == Some(archive_revision) {
        return;
    }
    localization_precedence_index.localization_asset_handles = archives
        .resolved_paths()
        .1
        .iter()
        .filter(|path| source_path_contains_english_localization(&path.to_string_lossy()))
        .map(|path| asset_server.load(path.clone()))
        .collect();
    localization_precedence_index
        .localization_entry_asset_index
        .clear();
    localization_precedence_index.localization_index_is_dirty = true;
    localization_precedence_index.indexed_archive_revision = Some(archive_revision);
}

pub(super) fn index_loaded_localization_entries_after_asset_events(
    mut localization_asset_events: MessageReader<AssetEvent<LocalizationAsset>>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut localization_precedence_index: ResMut<LocalizationPrecedenceIndex>,
) {
    if localization_asset_events
        .read()
        .any(|event| !matches!(event, AssetEvent::Unused { .. }))
    {
        localization_precedence_index.localization_index_is_dirty = true;
    }
    if !localization_precedence_index.localization_index_is_dirty
        || localization_precedence_index
            .localization_asset_handles
            .iter()
            .any(|handle| localization_assets.get(handle).is_none())
    {
        return;
    }
    localization_precedence_index.localization_entry_asset_index = localization_precedence_index
        .localization_asset_handles
        .iter()
        .filter_map(|handle| localization_assets.get(handle).map(|asset| (handle, asset)))
        .flat_map(|(handle, asset)| {
            asset
                .localization_catalog
                .localization_entries
                .iter()
                .map(move |entry| (entry.localization_entry_identifier, handle.clone()))
        })
        .collect();
    localization_precedence_index.localization_index_is_dirty = false;
}

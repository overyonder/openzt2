use std::collections::BTreeMap;

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::AssetId;

use super::{
    blue_fang_localization_locale_recognition::source_path_contains_english_localization,
    loaded_localization_queries::LoadedLocalizationView,
    localization_asset_types::LocalizationAsset,
};

#[derive(Resource, Default)]
pub struct LocalizationPrecedenceIndex {
    indexed_archive_revision: Option<u64>,
    localization_index_is_dirty: bool,
    localization_asset_handles: Vec<Handle<LocalizationAsset>>,
    localization_entry_asset_index: BTreeMap<AssetId, Handle<LocalizationAsset>>,
}

impl LocalizationPrecedenceIndex {
    #[cfg(any(test, feature = "testing"))]
    pub fn index_test_catalogue(
        &mut self,
        assets: &mut Assets<LocalizationAsset>,
        catalogue: openzt2_game_data::localization::LocalizationCatalog,
    ) {
        let identifiers = catalogue
            .localization_entries
            .iter()
            .map(|entry| entry.localization_entry_identifier)
            .collect::<Vec<_>>();
        let handle = assets.add(LocalizationAsset::from_localization_catalog(catalogue));
        self.localization_entry_asset_index
            .extend(identifiers.into_iter().map(|id| (id, handle.clone())));
        self.localization_asset_handles.push(handle);
        self.indexed_archive_revision.get_or_insert(0);
        self.localization_index_is_dirty = false;
    }

    pub fn has_pending_catalogue_loads(
        &self,
        asset_server: &AssetServer,
        assets: &Assets<LocalizationAsset>,
    ) -> bool {
        self.indexed_archive_revision.is_none()
            || self.localization_index_is_dirty
            || self.has_unsettled_assets(asset_server, assets)
    }

    fn has_unsettled_assets(
        &self,
        server: &AssetServer,
        assets: &Assets<LocalizationAsset>,
    ) -> bool {
        self.localization_asset_handles.iter().any(|handle| {
            !assets.contains(handle.id())
                && !matches!(
                    server.get_load_state(handle.id()),
                    Some(LoadState::Failed(_))
                )
        })
    }

    pub fn borrow_loaded_localization_view<'a>(
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
    mut failures: MessageReader<AssetLoadFailedEvent<LocalizationAsset>>,
    server: Res<AssetServer>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut localization_precedence_index: ResMut<LocalizationPrecedenceIndex>,
) {
    let assets_changed = localization_asset_events
        .read()
        .any(|event| !matches!(event, AssetEvent::Unused { .. }));
    let failed = failures.read().count() != 0;
    if assets_changed || failed {
        localization_precedence_index.localization_index_is_dirty = true;
    }
    if !localization_precedence_index.localization_index_is_dirty
        || localization_precedence_index.has_unsettled_assets(&server, &localization_assets)
    {
        return;
    }
    localization_precedence_index.localization_entry_asset_index = localization_precedence_index
        .localization_asset_handles
        .iter()
        .filter(|handle| {
            !matches!(
                server.get_load_state(handle.id()),
                Some(LoadState::Failed(_))
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        asset_source::{create_bevy_asset_source_for_enabled_z2f_archives, tests::TestDirectory},
        localization::LocalizationAssetPlugin,
    };
    use bevy::{
        app::TaskPoolPlugin,
        asset::{io::AssetSourceId, AssetPlugin},
    };
    use std::{
        io,
        time::{Duration, Instant},
    };

    #[test]
    fn malformed_catalogue_does_not_hide_valid_localisation() -> io::Result<()> {
        let directory = TestDirectory::new()?;
        let archive = directory.archive("localisation.z2f", &[
            ("lang/1033/good_strings.xml", br#"<ZT2Strings><LOC_STRING _locID="mainmenu:profile_default">Default</LOC_STRING></ZT2Strings>"#),
            ("lang/1033/bad_strings.xml", b"<"),
        ])?;
        let (source, archives) = create_bevy_asset_source_for_enabled_z2f_archives(vec![archive])?;
        let mut app = App::new();
        app.register_asset_source(AssetSourceId::Default, source)
            .insert_resource(archives)
            .add_plugins((
                TaskPoolPlugin::default(),
                AssetPlugin::default(),
                LocalizationAssetPlugin,
            ));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update();
            let world = app.world();
            let server = world.resource::<AssetServer>();
            let index = world.resource::<LocalizationPrecedenceIndex>();
            let assets = world.resource::<Assets<LocalizationAsset>>();
            if !index.has_pending_catalogue_loads(server, assets) {
                let view = index.borrow_loaded_localization_view(assets).unwrap();
                assert_eq!(
                    view.find_plain_localized_text(AssetId::from_key("mainmenu:profile_default")),
                    Some("Default")
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("localisation catalogue never settled"));
            }
            std::thread::yield_now();
        }
    }
}

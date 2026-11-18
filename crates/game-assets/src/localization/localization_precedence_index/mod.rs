use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{
    asset::{AssetId as BevyAssetId, AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::{AssetId, localization::LocalizationDateFormatToken};

use crate::asset_source::{ArchiveOverlayChanged, AssetArchives};

use super::{
    blue_fang_localization_date_format_tokenization::tokenize_blue_fang_localization_date_format,
    loaded_localization_queries::LoadedLocalizationView,
    localization_asset_types::LocalizationAsset,
    localization_configuration::{LOCALE_CONFIGURATION_PATH, LocalizationConfiguration},
};

#[derive(Resource, Default)]
pub struct LocalizationPrecedenceIndex {
    configuration_handle: Option<Handle<LocalizationConfiguration>>,
    configuration: LocalizationConfiguration,
    discovered: bool,
    pending: BTreeSet<BevyAssetId<LocalizationAsset>>,
    failed: BTreeSet<BevyAssetId<LocalizationAsset>>,
    localization_asset_handles: Vec<Handle<LocalizationAsset>>,
    localization_entry_asset_index: BTreeMap<AssetId, Handle<LocalizationAsset>>,
    month_year_tokens: Vec<LocalizationDateFormatToken>,
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
        self.discovered = true;
        self.update_month_year_format(assets);
    }

    pub fn has_pending_catalogue_loads(
        &self,
        _asset_server: &AssetServer,
        _assets: &Assets<LocalizationAsset>,
    ) -> bool {
        !self.discovered || !self.pending.is_empty()
    }

    pub fn borrow_loaded_localization_view<'a>(
        &'a self,
        assets: &'a Assets<LocalizationAsset>,
    ) -> Option<LoadedLocalizationView<'a>> {
        (self.discovered && self.pending.is_empty() && !self.localization_asset_handles.is_empty())
            .then_some(LoadedLocalizationView::new(self, assets))
    }

    pub(super) fn find_asset_for_localization_entry<'a>(
        &self,
        assets: &'a Assets<LocalizationAsset>,
        identifier: AssetId,
    ) -> Option<&'a LocalizationAsset> {
        assets.get(self.localization_entry_asset_index.get(&identifier)?)
    }

    pub(super) fn configuration(&self) -> &LocalizationConfiguration {
        &self.configuration
    }

    pub(super) fn month_year_tokens(&self) -> &[LocalizationDateFormatToken] {
        &self.month_year_tokens
    }

    fn update_month_year_format(&mut self, assets: &Assets<LocalizationAsset>) {
        let format_id = self.configuration.month_year;
        self.month_year_tokens = self
            .find_asset_for_localization_entry(assets, format_id)
            .and_then(|asset| {
                asset
                    .localization_catalog
                    .find_plain_localized_text(format_id)
            })
            .map_or_else(Vec::new, tokenize_blue_fang_localization_date_format);
    }
}

pub(super) fn index_loaded_localization_entries_after_asset_events(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    configurations: Res<Assets<LocalizationConfiguration>>,
    assets: Res<Assets<LocalizationAsset>>,
    mut events: MessageReader<AssetEvent<LocalizationAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<LocalizationAsset>>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
    default_configuration: Local<LocalizationConfiguration>,
    mut index: ResMut<LocalizationPrecedenceIndex>,
) {
    let configuration_path = Path::new(LOCALE_CONFIGURATION_PATH);
    let mut discovery_changed = false;
    if archives.is_changed() {
        let handle = archives
            .source_exists(configuration_path)
            .then(|| server.load(configuration_path));
        if index.configuration_handle != handle {
            index.configuration_handle = handle;
            discovery_changed = true;
        }
    }
    let mut catalogue_sources_changed = false;
    for change in overlay_changes.read().flat_map(|overlay| &overlay.changes) {
        catalogue_sources_changed |= index
            .configuration
            .selects_string_table(&change.normalized_asset_path);
    }
    let configuration = if let Some(handle) = index.configuration_handle.as_ref() {
        if matches!(
            server.get_load_state(handle.id()),
            Some(LoadState::Failed(_))
        ) {
            &*default_configuration
        } else if let Some(configuration) = configurations.get(handle) {
            configuration
        } else {
            for _ in events.read() {}
            for _ in failures.read() {}
            return;
        }
    } else {
        &*default_configuration
    };
    if !index.discovered
        || discovery_changed
        || catalogue_sources_changed
        || index.configuration != *configuration
    {
        let handles = archives
            .resolved_paths()
            .1
            .iter()
            .filter(|path| configuration.selects_string_table(path))
            .map(|path| server.load(path.clone()))
            .collect::<Vec<_>>();
        if !index.discovered
            || index.localization_asset_handles != handles
            || index.configuration != *configuration
        {
            index.pending = handles
                .iter()
                .map(Handle::id)
                .filter(|id| {
                    !assets.contains(*id)
                        && !matches!(server.get_load_state(*id), Some(LoadState::Failed(_)))
                })
                .collect();
            index.failed = handles
                .iter()
                .map(Handle::id)
                .filter(|id| matches!(server.get_load_state(*id), Some(LoadState::Failed(_))))
                .collect();
            index.localization_asset_handles = handles;
            index.configuration = configuration.clone();
            index.discovered = true;
            discovery_changed = true;
        }
    }
    let mut changed = discovery_changed;
    for event in events.read() {
        let id = match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                id
            }
            AssetEvent::Unused { .. } | AssetEvent::LoadedWithDependencies { .. } => continue,
        };
        if index
            .localization_asset_handles
            .iter()
            .any(|handle| handle.id() == id)
        {
            changed = true;
            index.pending.remove(&id);
            index.failed.remove(&id);
        }
    }
    for failure in failures.read() {
        if index
            .localization_asset_handles
            .iter()
            .any(|handle| handle.id() == failure.id)
        {
            changed |= index.pending.remove(&failure.id);
            changed |= index.failed.insert(failure.id);
        }
    }
    if !changed || !index.pending.is_empty() {
        return;
    }
    index.localization_entry_asset_index = index
        .localization_asset_handles
        .iter()
        .filter(|handle| !index.failed.contains(&handle.id()))
        .filter_map(|handle| assets.get(handle).map(|asset| (handle, asset)))
        .flat_map(|(handle, asset)| {
            asset
                .localization_catalog
                .localization_entries
                .iter()
                .map(move |entry| (entry.localization_entry_identifier, handle.clone()))
        })
        .collect();
    index.update_month_year_format(&assets);
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
        asset::{AssetPlugin, io::AssetSourceId},
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

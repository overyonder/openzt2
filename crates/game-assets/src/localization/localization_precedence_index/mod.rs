use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{
    asset::{AssetId as BevyAssetId, AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::AssetId;

use crate::asset_source::{
    ArchiveOverlayChanged, AssetArchives, archive_entry_asset_path::archive_entry_asset_path,
};

use super::{
    blue_fang_localization_date_format_tokenization::{
        LocalizationDateTimeFormat, LocalizationDateTimeToken,
        tokenize_blue_fang_localization_date_format,
    },
    loaded_localization_queries::LoadedLocalizationView,
    localization_asset_loading::LocalizationTableSettings,
    localization_asset_types::LocalizationAsset,
    localization_configuration::{LOCALE_CONFIGURATION_PATH, LocalizationConfiguration},
};

#[derive(Resource, Default)]
pub struct LocalizationPrecedenceIndex {
    configuration_handle: Option<Handle<LocalizationConfiguration>>,
    configuration: LocalizationConfiguration,
    discovered: bool,
    selected_table_ids: BTreeSet<BevyAssetId<LocalizationAsset>>,
    pending: BTreeSet<BevyAssetId<LocalizationAsset>>,
    failed: BTreeSet<BevyAssetId<LocalizationAsset>>,
    localization_asset_handles: Vec<Handle<LocalizationAsset>>,
    person_name_pool_asset_index: BTreeMap<AssetId, Handle<LocalizationAsset>>,
    localization_entry_asset_index: BTreeMap<AssetId, Handle<LocalizationAsset>>,
    date_time_formats: [Vec<LocalizationDateTimeToken>; 4],
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
        self.selected_table_ids.insert(handle.id());
        self.localization_asset_handles.push(handle);
        self.discovered = true;
        self.update_date_time_formats(assets);
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

    pub(super) fn find_person_name_pool<'a>(
        &self,
        assets: &'a Assets<LocalizationAsset>,
        id: AssetId,
    ) -> Option<
        &'a openzt2_game_data::world_definitions::guest_simulation_definitions::PersonNamePool,
    > {
        assets
            .get(self.person_name_pool_asset_index.get(&id)?)?
            .person_name_pools
            .get(&id)
    }

    pub(super) fn configuration(&self) -> &LocalizationConfiguration {
        &self.configuration
    }

    pub(super) fn date_time_tokens(
        &self,
        format: LocalizationDateTimeFormat,
    ) -> &[LocalizationDateTimeToken] {
        &self.date_time_formats[format as usize]
    }

    fn update_date_time_formats(&mut self, assets: &Assets<LocalizationAsset>) {
        self.date_time_formats = std::array::from_fn(|index| {
            let id = self.configuration.date_time_formats[index];
            let pattern = self
                .find_asset_for_localization_entry(assets, id)
                .and_then(|asset| asset.localization_catalog.find_plain_localized_text(id))
                .filter(|pattern| !pattern.is_empty());
            let time = index == LocalizationDateTimeFormat::MinutesSeconds as usize;
            let calendar = &self.configuration.locale_conventions().calendar;
            let pattern = pattern.unwrap_or(if time {
                &calendar.time_format
            } else {
                &calendar.short_date
            });
            tokenize_blue_fang_localization_date_format(pattern, time)
        });
    }
}

pub(super) fn index_loaded_localization_entries_after_asset_events(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    configurations: Res<Assets<LocalizationConfiguration>>,
    assets: Res<Assets<LocalizationAsset>>,
    (mut configuration_events, mut configuration_failures): (
        MessageReader<AssetEvent<LocalizationConfiguration>>,
        MessageReader<AssetLoadFailedEvent<LocalizationConfiguration>>,
    ),
    mut events: MessageReader<AssetEvent<LocalizationAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<LocalizationAsset>>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
    default_configuration: Local<LocalizationConfiguration>,
    mut index: ResMut<LocalizationPrecedenceIndex>,
) {
    let configuration_path = Path::new(LOCALE_CONFIGURATION_PATH);
    let mut configuration_changed = false;
    if archives.is_changed() {
        let path = archives
            .resolved_winning_entry(configuration_path)
            .map_or_else(
                || configuration_path.to_owned(),
                |entry| archive_entry_asset_path(&entry),
            );
        let handle = archives
            .source_exists(configuration_path)
            .then(|| server.load(path));
        if index.configuration_handle != handle {
            index.configuration_handle = handle;
            configuration_changed = true;
        }
    }
    let current_configuration = index.configuration_handle.as_ref().map(Handle::id);
    for event in configuration_events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                configuration_changed |= current_configuration == Some(id);
            }
            AssetEvent::Unused { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
    for failure in configuration_failures.read() {
        configuration_changed |= current_configuration == Some(failure.id);
    }
    let mut catalogue_sources_changed = false;
    for change in overlay_changes.read().flat_map(|overlay| &overlay.changes) {
        catalogue_sources_changed |= index
            .configuration
            .selects_string_table(&change.normalized_asset_path);
    }
    let mut discovery_changed = false;
    if configuration_changed
        || catalogue_sources_changed
        || (!index.discovered && current_configuration.is_none())
    {
        let selected = match current_configuration {
            None => Some(&*default_configuration),
            Some(id) if matches!(server.get_load_state(id), Some(LoadState::Failed(_))) => {
                Some(&*default_configuration)
            }
            Some(id) => configurations.get(id),
        };
        if let Some(configuration) = selected {
            let mut loose = archives
                .source_paths_under(configuration.string_table_root())
                .into_iter()
                .filter(|path| configuration.selects_string_table(path))
                .collect::<BTreeSet<_>>();
            let load_table = |source_path: &Path, asset_path| {
                let locale = configuration.table_locale(source_path)?.to_owned();
                Some(
                    server
                        .load_builder()
                        .with_settings(move |settings: &mut LocalizationTableSettings| {
                            settings.locale.clone_from(&locale);
                        })
                        .load::<LocalizationAsset>(asset_path),
                )
            };
            let mut handles = archives
                .resolved_archive_entries()
                .iter()
                .filter(|entry| loose.remove(&entry.normalized_asset_path))
                .filter_map(|entry| {
                    load_table(
                        &entry.normalized_asset_path,
                        archive_entry_asset_path(entry),
                    )
                })
                .collect::<Vec<_>>();
            handles.extend(
                loose
                    .into_iter()
                    .filter_map(|path| load_table(&path, path.clone())),
            );
            if !index.discovered
                || index.localization_asset_handles != handles
                || index.configuration != *configuration
            {
                index.pending.clear();
                index.failed.clear();
                for id in handles.iter().map(Handle::id) {
                    match server.get_load_state(id) {
                        Some(LoadState::Failed(_)) => {
                            index.failed.insert(id);
                        }
                        Some(LoadState::Loaded) if assets.contains(id) => {}
                        _ => {
                            index.pending.insert(id);
                        }
                    }
                }
                index.selected_table_ids = handles.iter().map(Handle::id).collect();
                index.localization_asset_handles = handles;
                index.configuration = configuration.clone();
                index.discovered = true;
                discovery_changed = true;
            }
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
        if index.selected_table_ids.contains(&id) {
            changed = true;
            index.pending.remove(&id);
            index.failed.remove(&id);
        }
    }
    for failure in failures.read() {
        if index.selected_table_ids.contains(&failure.id) {
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
    index.person_name_pool_asset_index = index
        .localization_asset_handles
        .iter()
        .filter(|handle| !index.failed.contains(&handle.id()))
        .filter_map(|handle| assets.get(handle).map(|asset| (handle, asset)))
        .flat_map(|(handle, asset)| {
            asset
                .person_name_pools
                .keys()
                .map(move |id| (*id, handle.clone()))
        })
        .collect();
    index.update_date_time_formats(&assets);
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

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::audio::VARIANT_CUE_REFERENCE;

use crate::asset_source::{ArchiveCatalogue, AssetArchives};

use super::audio_asset_types::{AudioAsset, AudioAssets, AudioCatalogueSource, AudioRowKey};

fn path_belongs_to_audio_document_family(path: &Path) -> bool {
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    path.starts_with("config/sound")
        || path.starts_with("entities/sound/")
        || path.starts_with("ui/sound/")
        || path.starts_with("world/sound/")
        || path.starts_with("world/soundtags/")
        || path.starts_with("world/ambients/")
        || path == "ai/ambientsmgr.xml"
}

pub(crate) fn is_audio_catalogue_source(path: &Path) -> bool {
    path_belongs_to_audio_document_family(path)
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

pub(super) fn discover_audio_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<AudioAsset>>,
    mut index: ResMut<AudioAssets>,
    mut previous: Local<Option<Arc<[z2f::ResolvedArchiveEntry]>>>,
) {
    let sources = archives.catalogue_sources(ArchiveCatalogue::Audio);
    if previous
        .as_ref()
        .is_some_and(|previous| Arc::ptr_eq(previous, &sources))
    {
        return;
    }
    let index = &mut *index;
    let mut old_sources = std::mem::take(&mut index.sources);
    for entry in sources.iter() {
        let path = &entry.normalized_asset_path;
        let rank = (entry.archive_priority_index, entry.zip_entry_ordinal);
        let new_source = !old_sources.contains_key(path);
        let mut source = old_sources.remove(path).map_or_else(
            || AudioCatalogueSource {
                handle: server.load::<AudioAsset>(path.clone()),
                rank,
                rows: BTreeMap::new(),
                biomes: BTreeMap::new(),
            },
            |mut existing| {
                if existing.rank != rank {
                    index.affected_keys.extend(existing.rows.keys().copied());
                    existing.rank = rank;
                    existing.rows.clear();
                    existing.biomes.clear();
                }
                existing
            },
        );
        let winner_changed = previous.as_ref().is_some_and(|previous| {
            previous
                .iter()
                .any(|old| old.normalized_asset_path == *path && old != entry)
        });
        if winner_changed
            || !matches!(
                server.get_load_state(source.handle.id()),
                Some(LoadState::Loaded | LoadState::Failed(_))
            )
        {
            index.pending.insert(source.handle.id());
        }
        if new_source && !index.pending.contains(&source.handle.id()) {
            if let Some(asset) = loaded_audio_document(&source, &assets, &server) {
                let directory_biome = path
                    .parent()
                    .and_then(Path::to_str)
                    .and_then(|directory| index.biome_directories.get(directory).copied());
                source.rows = audio_document_rows(asset, directory_biome);
                source
                    .biomes
                    .extend(asset.document.soundscape_biomes.iter().map(|biome| {
                        (
                            biome.directory.trim_end_matches('/').to_ascii_lowercase(),
                            biome.biome,
                        )
                    }));
                index.affected_keys.extend(source.rows.keys().copied());
            }
        }
        index.sources.insert(path.clone(), source);
    }
    for source in old_sources.into_values() {
        index.pending.remove(&source.handle.id());
        index.affected_keys.extend(source.rows.into_keys());
    }
    index.discovered = true;
    *previous = Some(sources);
}

pub(super) fn publish_loaded_audio_rows(
    mut events: MessageReader<AssetEvent<AudioAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<AudioAsset>>,
    server: Res<AssetServer>,
    assets: Res<Assets<AudioAsset>>,
    mut index: ResMut<AudioAssets>,
) {
    let mut changed_paths = BTreeSet::new();
    for event in events.read() {
        let id = match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                id
            }
            AssetEvent::LoadedWithDependencies { .. } | AssetEvent::Unused { .. } => continue,
        };
        if let Some(path) = server.get_path(id) {
            if index
                .sources
                .get(path.path())
                .is_some_and(|source| source.handle.id() == id)
            {
                index.pending.remove(&id);
                changed_paths.insert(path.path().to_owned());
            }
        }
    }
    for failure in failures.read() {
        if index
            .sources
            .get(failure.path.path())
            .is_some_and(|source| source.handle.id() == failure.id)
        {
            index.pending.remove(&failure.id);
            changed_paths.insert(failure.path.path().to_owned());
        }
    }
    if changed_paths.is_empty() && index.affected_keys.is_empty() {
        return;
    }
    let index = &mut *index;
    for path in &changed_paths {
        let Some(source) = index.sources.get_mut(path) else {
            continue;
        };
        source.biomes.clear();
        if let Some(asset) = loaded_audio_document(source, &assets, &server) {
            source
                .biomes
                .extend(asset.document.soundscape_biomes.iter().map(|biome| {
                    (
                        biome.directory.trim_end_matches('/').to_ascii_lowercase(),
                        biome.biome,
                    )
                }));
        }
    }
    let biome_directories = index
        .sources
        .values()
        .flat_map(|source| {
            source
                .biomes
                .iter()
                .map(|(path, biome)| (path, (source.rank, *biome)))
        })
        .fold(BTreeMap::new(), |mut directories, (path, row)| {
            let winner = directories.entry(path).or_insert(row);
            if winner.0 <= row.0 {
                *winner = row;
            }
            directories
        })
        .into_iter()
        .map(|(path, (_, biome))| (path.clone(), biome))
        .collect::<BTreeMap<_, _>>();
    if biome_directories != index.biome_directories {
        changed_paths.extend(index.sources.iter().filter_map(|(path, source)| {
            let directory = path.parent()?.to_str()?;
            (biome_directories.get(directory) != index.biome_directories.get(directory)
                && !index.pending.contains(&source.handle.id())
                && loaded_audio_document(source, &assets, &server)
                    .is_some_and(|asset| !asset.document.soundscape_contents.is_empty()))
            .then(|| path.clone())
        }));
        index.biome_directories = biome_directories;
    }
    for path in &changed_paths {
        let Some(source) = index.sources.get_mut(path) else {
            continue;
        };
        index.affected_keys.extend(source.rows.keys().copied());
        let directory_biome = path
            .parent()
            .and_then(Path::to_str)
            .and_then(|directory| index.biome_directories.get(directory).copied());
        source.rows = loaded_audio_document(source, &assets, &server)
            .map_or_else(BTreeMap::new, |asset| {
                audio_document_rows(asset, directory_biome)
            });
        index.affected_keys.extend(source.rows.keys().copied());
    }
    if !index.pending.is_empty() {
        return;
    }
    for key in std::mem::take(&mut index.affected_keys) {
        if let Some((source, row)) = index
            .sources
            .values()
            .filter_map(|source| Some((source, *source.rows.get(&key)?)))
            .max_by_key(|(source, _)| source.rank)
        {
            index.winners.insert(key, (source.handle.clone(), row));
        } else {
            index.winners.remove(&key);
        }
    }
    for path in &changed_paths {
        let source = &index.sources[path];
        let Some(asset) = loaded_audio_document(source, &assets, &server) else {
            continue;
        };
        for cue in &asset.document.cues {
            for variant in &cue.variants {
                if variant.flags & VARIANT_CUE_REFERENCE != 0
                    && !index
                        .winners
                        .contains_key(&AudioRowKey::Cue(variant.source_id))
                {
                    warn!(
                        cue = cue.name,
                        referenced_cue = variant.source,
                        "Audio cue is unavailable; this variant cannot play"
                    );
                }
            }
        }
    }
}

fn loaded_audio_document<'a>(
    source: &AudioCatalogueSource,
    assets: &'a Assets<AudioAsset>,
    server: &AssetServer,
) -> Option<&'a AudioAsset> {
    if matches!(
        server.get_load_state(source.handle.id()),
        Some(LoadState::Failed(_))
    ) {
        return None;
    }
    assets.get(&source.handle)
}

fn audio_document_rows(
    asset: &AudioAsset,
    directory_biome: Option<openzt2_game_data::AssetId>,
) -> BTreeMap<AudioRowKey, usize> {
    let document = &asset.document;
    let mut rows = BTreeMap::new();
    for (index, cue) in document.cues.iter().enumerate() {
        rows.entry(AudioRowKey::Cue(cue.id)).or_insert(index);
    }
    for (index, stage) in document.stages.iter().enumerate() {
        rows.entry(AudioRowKey::Stage(stage.id)).or_insert(index);
    }
    for (index, allowed) in document.soundscape_allowed_entities.iter().enumerate() {
        rows.entry(AudioRowKey::AllowedEntities(allowed.biome))
            .or_insert(index);
    }
    for (index, soundscape) in document.soundscape_contents.iter().enumerate() {
        rows.insert(
            AudioRowKey::Soundscape(directory_biome.unwrap_or(soundscape.biome)),
            index,
        );
    }
    for (key, present) in [
        (AudioRowKey::Mixer, document.mixer.is_some()),
        (AudioRowKey::Ambient, document.ambient.is_some()),
        (AudioRowKey::Water, document.water.is_some()),
        (AudioRowKey::Crowd, document.crowd.is_some()),
    ] {
        if present {
            rows.insert(key, 0);
        }
    }
    rows
}

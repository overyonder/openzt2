use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};

use crate::asset_source::{
    archive_entry_asset_path::archive_entry_asset_path, ArchiveCatalogue, ArchiveOverlayChanged,
    AssetArchives,
};
use std::{collections::BTreeSet, path::Path};

use super::species_asset_types::{SpeciesAsset, SpeciesAssets};

pub(crate) fn is_species_catalogue_source(path: &Path) -> bool {
    path.starts_with("entities/units/animals/ai")
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

pub(super) fn discover_species_catalogue_sources(
    archives: Res<AssetArchives>,
    mut index: ResMut<SpeciesAssets>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) {
    let changed = overlay_changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| is_species_catalogue_source(&change.normalized_asset_path));
    overlay_changes.clear();
    if index.discovered && !changed {
        return;
    }
    let sources = archives.catalogue_sources(ArchiveCatalogue::Species);
    index.pending_paths = sources.iter().map(archive_entry_asset_path).collect();
    index.handles.clear();
    index.source_ids.clear();
    index.pending.clear();
    index.failed.clear();
    index.discovered = true;
    index.source_set_changed = true;
}

pub(super) fn index_loaded_species_assets_by_species_and_variant_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<SpeciesAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<SpeciesAsset>>,
    mut events: MessageReader<AssetEvent<SpeciesAsset>>,
    mut index: ResMut<SpeciesAssets>,
) {
    let affected = |id| index.source_ids.contains(&id);
    let mut settled = BTreeSet::new();
    let mut failed = BTreeSet::new();
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                if affected(id) {
                    settled.insert(id);
                }
            }
            AssetEvent::Unused { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
    for failure in failures.read() {
        if affected(failure.id) {
            failed.insert(failure.id);
        }
    }
    let changed = !settled.is_empty() || !failed.is_empty();
    if changed {
        for id in settled {
            index.pending.remove(&id);
            index.failed.remove(&id);
        }
        for id in failed {
            index.pending.remove(&id);
            index.failed.insert(id);
        }
    } else if !index.source_set_changed || !index.pending.is_empty() {
        return;
    }
    if index.source_set_changed {
        while index.pending.is_empty() && !index.pending_paths.is_empty() {
            request_species_batch(&server, &assets, &mut index);
        }
        if !index.pending.is_empty() {
            return;
        }
    }
    let documents = index
        .handles
        .iter()
        .filter(|handle| !index.failed.contains(&handle.id()))
        .filter_map(|handle| {
            let asset = assets.get(handle)?;
            Some((
                handle.clone(),
                &asset.document,
                asset.animation_sets.as_ref(),
            ))
        })
        .collect::<Vec<_>>();
    index.index_loaded_documents(&documents);
    index.source_set_changed = false;
}

fn request_species_batch(
    server: &AssetServer,
    assets: &Assets<SpeciesAsset>,
    species: &mut SpeciesAssets,
) {
    const SPECIES_BATCH: usize = 4;
    for path in species
        .pending_paths
        .drain(..species.pending_paths.len().min(SPECIES_BATCH))
    {
        let handle = server.load::<SpeciesAsset>(path);
        match server.get_load_state(handle.id()) {
            Some(LoadState::Failed(_)) => {
                species.failed.insert(handle.id());
            }
            Some(LoadState::Loaded) if assets.contains(handle.id()) => {}
            _ => {
                species.pending.insert(handle.id());
            }
        }
        species.source_ids.insert(handle.id());
        species.handles.push(handle);
    }
}

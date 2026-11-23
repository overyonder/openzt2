use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};

use crate::asset_source::{
    ArchiveOverlayChanged, AssetArchives,
    archive_entry_asset_path::definition_archive_entry_asset_path,
};
use std::{collections::BTreeSet, path::Path};

use super::species_asset_types::{SpeciesAsset, SpeciesAssets};

pub(super) fn discover_species_catalogue_sources(
    archives: Res<AssetArchives>,
    mut index: ResMut<SpeciesAssets>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) {
    let binders = archives.authored_binder_sources();
    let changed = overlay_changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| {
            change.normalized_asset_path == Path::new("config/gamectxt.xml")
                || binders.contains_path(&change.normalized_asset_path)
        });
    overlay_changes.clear();
    if !changed
        && index
            .binder_sources
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(old, &binders))
    {
        return;
    }
    let animal_paths = match binders.animal_binder_paths(&archives) {
        Ok(paths) => paths,
        Err(error) => {
            error!(%error, "could not discover animal binders");
            BTreeSet::new()
        }
    };
    index.source_generation += 1;
    let generation = index.source_generation;
    let mut loose = animal_paths;
    let mut paths = archives
        .resolved_archive_entries()
        .iter()
        .filter(|entry| loose.remove(&entry.normalized_asset_path))
        .map(|entry| definition_archive_entry_asset_path(entry, generation))
        .collect::<std::collections::VecDeque<_>>();
    paths.extend(loose);
    index.pending_paths = paths;
    index.handles.clear();
    index.source_ids.clear();
    index.pending.clear();
    index.failed.clear();
    index.binder_sources = Some(binders);
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

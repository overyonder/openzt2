use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};

use crate::asset_source::{ArchiveCatalogue, AssetArchives};
use std::{path::Path, sync::Arc};

use super::species_asset_types::{SpeciesAsset, SpeciesAssets, VariantHandle};

pub(crate) fn is_species_catalogue_source(path: &Path) -> bool {
    path.starts_with("entities/units/animals/ai")
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

pub(super) fn discover_species_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut index: ResMut<SpeciesAssets>,
    mut previous: Local<Option<Arc<[z2f::ResolvedArchiveEntry]>>>,
) {
    if !archives.is_changed() && previous.is_some() {
        return;
    }
    let sources = archives.catalogue_sources(ArchiveCatalogue::Species);
    if previous
        .as_ref()
        .is_some_and(|previous| Arc::ptr_eq(previous, &sources))
    {
        return;
    }
    let current_paths = sources.iter().map(|entry| &entry.normalized_asset_path);
    let same_paths = previous.as_ref().is_some_and(|previous| {
        previous
            .iter()
            .map(|entry| &entry.normalized_asset_path)
            .eq(current_paths)
    });
    if !same_paths {
        index.pending_paths = sources
            .iter()
            .map(|entry| entry.normalized_asset_path.clone())
            .collect();
        index.handles.clear();
        index.loaded_handles.clear();
        request_species_batch(&server, &mut index);
        index.discovered = true;
        index.dirty = true;
    }
    *previous = Some(sources);
}

pub(super) fn index_loaded_species_assets_by_species_and_variant_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<SpeciesAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<SpeciesAsset>>,
    mut events: MessageReader<AssetEvent<SpeciesAsset>>,
    mut index: ResMut<SpeciesAssets>,
) {
    let affected = |id| index.handles.iter().any(|handle| handle.id() == id);
    let mut changed = false;
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                changed |= affected(id);
            }
            AssetEvent::Unused { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
    for failure in failures.read() {
        changed |= affected(failure.id);
    }
    if index.dirty {
        if !species_batch_finished(&server, &index) {
            return;
        }
        if !index.pending_paths.is_empty() {
            request_species_batch(&server, &mut index);
            return;
        }
    } else if !changed {
        return;
    }
    let loaded_handles = index
        .handles
        .iter()
        .filter(|handle| {
            assets.contains(handle.id())
                && !matches!(
                    server.get_load_state(handle.id()),
                    Some(LoadState::Failed(_))
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    let by_species = loaded_handles
        .iter()
        .filter_map(|handle| assets.get(handle).map(|asset| (handle, asset)))
        .flat_map(|(handle, asset)| {
            asset
                .document
                .species
                .iter()
                .map(move |species| (species.id, handle.clone()))
        })
        .collect();
    let by_variant = loaded_handles
        .iter()
        .filter_map(|handle| assets.get(handle).map(|asset| (handle, asset)))
        .flat_map(|(handle, asset)| {
            asset
                .document
                .variants
                .iter()
                .enumerate()
                .map(move |(index, binding)| {
                    (
                        binding.variant.id,
                        VariantHandle {
                            species: binding.species,
                            document: handle.clone(),
                            index,
                        },
                    )
                })
        })
        .collect();
    index.by_species = by_species;
    index.by_variant = by_variant;
    index.rebuild_variant_indexes(loaded_handles.iter().filter_map(|handle| {
        let asset = assets.get(handle)?;
        Some((handle, &asset.document, asset.animation_sets.as_ref()))
    }));
    index.loaded_handles = loaded_handles;
    index.dirty = false;
}

const SPECIES_BATCH: usize = 4;

fn species_batch_finished(server: &AssetServer, species: &SpeciesAssets) -> bool {
    species
        .handles
        .iter()
        .rev()
        .take(SPECIES_BATCH)
        .all(|handle| {
            matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
            )
        })
}

fn request_species_batch(server: &AssetServer, species: &mut SpeciesAssets) {
    species.handles.extend(
        species
            .pending_paths
            .drain(..species.pending_paths.len().min(SPECIES_BATCH))
            .map(|path| server.load::<SpeciesAsset>(path)),
    );
}

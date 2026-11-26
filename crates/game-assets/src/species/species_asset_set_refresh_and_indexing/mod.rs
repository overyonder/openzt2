use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use std::{io, path::Path};

use crate::{
    asset_source::{
        ArchiveOverlayChanged, AssetArchives,
        archive_entry_asset_path::definition_archive_entry_asset_path,
    },
    world_definitions::world_definition_load_queue::load_asset_paths_in_order,
};

use super::species_asset_types::{SpeciesAsset, SpeciesAssets};

#[derive(Asset, TypePath)]
pub(super) struct SpeciesCatalogueLoad(Vec<Handle<SpeciesAsset>>);

pub(super) fn registered_animal_binder_sources_changed(
    archives: Res<AssetArchives>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) -> bool {
    let binders = archives.authored_binder_sources();
    let changed = overlay_changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| {
            change.normalized_asset_path == Path::new("config/gamectxt.xml")
                || binders.contains_path(&change.normalized_asset_path)
        });
    overlay_changes.clear();
    changed
}

pub(super) fn discover_species_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut index: ResMut<SpeciesAssets>,
) {
    let binders = archives.authored_binder_sources();
    let mut loose = binders
        .animal_binder_paths(&archives)
        .unwrap_or_else(|error| {
            error!(%error, "could not discover animal binders");
            Default::default()
        });
    let generation = archives.overlay_generation();
    let mut paths = archives
        .resolved_archive_entries()
        .iter()
        .filter(|entry| loose.remove(&entry.normalized_asset_path))
        .map(|entry| definition_archive_entry_asset_path(entry, generation))
        .collect::<Vec<_>>();
    paths.splice(..0, loose);
    let asset_server = server.clone();
    index.pending_catalogue = Some(server.add_async(async move {
        let handles =
            load_asset_paths_in_order::<SpeciesAsset>(asset_server, paths, Default::default())
                .await;
        Ok::<_, io::Error>(SpeciesCatalogueLoad(handles))
    }));
}

pub(super) fn index_loaded_species_assets_by_species_and_variant_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<SpeciesAsset>>,
    catalogues: Res<Assets<SpeciesCatalogueLoad>>,
    mut completed: MessageReader<AssetEvent<SpeciesCatalogueLoad>>,
    mut failures: MessageReader<AssetLoadFailedEvent<SpeciesAsset>>,
    mut events: MessageReader<AssetEvent<SpeciesAsset>>,
    mut index: ResMut<SpeciesAssets>,
) {
    let expected = index.pending_catalogue.as_ref().map(Handle::id);
    let catalogue = completed.read().find_map(|event| match event {
        AssetEvent::Added { id } if Some(*id) == expected => catalogues.get(*id),
        _ => None,
    });
    let changed = events.read().any(|event| match event {
        AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
            index.source_ids.contains(id)
        }
        _ => false,
    });
    let failed = failures
        .read()
        .any(|failure| index.source_ids.contains(&failure.id));
    events.clear();
    failures.clear();
    if let Some(catalogue) = catalogue {
        index.source_ids = catalogue.0.iter().map(Handle::id).collect();
        index.source_handles = Some(catalogue.0.clone());
        index.pending_catalogue = None;
    } else if expected.is_some() || !(changed || failed) {
        return;
    }
    let documents = index
        .source_handles
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|handle| {
            !matches!(
                server.get_load_state(handle.id()),
                Some(LoadState::Failed(_))
            )
        })
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
}

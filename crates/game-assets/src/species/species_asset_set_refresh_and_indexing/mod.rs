use bevy::prelude::*;

use crate::asset_source::AssetArchives;

use super::species_asset_types::{SpeciesAsset, SpeciesAssets, VariantHandle};

pub(super) fn refresh_species_asset_set_after_archive_revision(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut index: ResMut<SpeciesAssets>,
) {
    let revision = archives.revision();
    if index.revision == Some(revision) {
        return;
    }
    let paths = archives
        .resolved_paths()
        .1
        .iter()
        .cloned()
        .filter(|path| {
            let normalized = path
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            normalized.starts_with("entities/units/animals/ai/")
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        })
        .collect::<Vec<_>>();
    index.pending_paths = paths.into();
    index.handles.clear();
    index.complete = false;
    request_species_batch(&server, &mut index);
    index.revision = Some(revision);
    index.dirty = true;
}

pub(super) fn index_loaded_species_assets_by_species_and_variant_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<SpeciesAsset>>,
    mut index: ResMut<SpeciesAssets>,
) {
    if index.dirty {
        if !species_batch_finished(&server, &index) {
            return;
        }
        if !index.pending_paths.is_empty() {
            request_species_batch(&server, &mut index);
            return;
        }
    } else if !assets.is_changed() {
        return;
    }
    let by_species = index
        .handles
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
    let by_variant = index
        .handles
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
    index.complete = !index.handles.is_empty()
        && index
            .handles
            .iter()
            .all(|handle| assets.get(handle).is_some());
    index.dirty = false;
}

const SPECIES_BATCH: usize = 4;

fn species_batch_finished(server: &AssetServer, species: &SpeciesAssets) -> bool {
    !species.handles.is_empty()
        && species
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

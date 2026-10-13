use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::{species::SpeciesDocument, AssetId};

pub mod species_animation_set_loading;
pub mod species_asset_queries;

#[derive(Debug)]
pub(super) struct SpeciesAnimationSetDependencyPath {
    pub(super) id: AssetId,
    pub(super) path: Box<str>,
}

#[derive(Asset, TypePath, Debug)]
pub struct SpeciesAsset {
    pub(super) document: SpeciesDocument,
    pub(super) animation_sets: Box<[SpeciesAnimationSetDependencyPath]>,
}

#[cfg(any(test, feature = "testing"))]
impl SpeciesAsset {
    pub fn from_test_document(document: SpeciesDocument) -> Self {
        Self {
            document,
            animation_sets: Box::new([]),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl SpeciesAssets {
    pub fn index_test_document(
        &mut self,
        handle: Handle<SpeciesAsset>,
        document: &SpeciesDocument,
    ) {
        self.handles = vec![handle.clone()];
        self.by_species = document
            .species
            .iter()
            .map(|species| (species.id, handle.clone()))
            .collect();
        self.by_variant = document
            .variants
            .iter()
            .enumerate()
            .map(|(index, binding)| {
                (
                    binding.variant.id,
                    VariantHandle {
                        species: binding.species,
                        document: handle.clone(),
                        index,
                    },
                )
            })
            .collect();
        self.complete = true;
    }
}

#[derive(Resource, Default)]
pub struct SpeciesAssets {
    pub(super) revision: Option<u64>,
    pub(super) dirty: bool,
    pub(super) complete: bool,
    pub(super) pending_paths: std::collections::VecDeque<std::path::PathBuf>,
    pub(super) handles: Vec<Handle<SpeciesAsset>>,
    pub(super) by_species: BTreeMap<AssetId, Handle<SpeciesAsset>>,
    pub(super) by_variant: BTreeMap<AssetId, VariantHandle>,
}

#[derive(Clone, Debug)]
pub(super) struct VariantHandle {
    pub(super) species: AssetId,
    pub(super) document: Handle<SpeciesAsset>,
    pub(super) index: usize,
}

#[derive(Clone, Copy)]
pub struct SpeciesView<'a> {
    handles: &'a [Handle<SpeciesAsset>],
    by_species: &'a BTreeMap<AssetId, Handle<SpeciesAsset>>,
    by_variant: &'a BTreeMap<AssetId, VariantHandle>,
    assets: &'a Assets<SpeciesAsset>,
}

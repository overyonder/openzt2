use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::{species::SpeciesDocument, AssetId};

mod species_animation_set_loading;
mod species_asset_queries;

#[derive(Debug)]
pub(super) struct SpeciesAnimationSetDependencyPath {
    pub(super) id: AssetId,
    pub(super) path: Box<str>,
}

#[derive(Asset, TypePath, Debug)]
pub(crate) struct SpeciesAsset {
    pub(super) document: SpeciesDocument,
    pub(super) animation_sets: Box<[SpeciesAnimationSetDependencyPath]>,
}

#[derive(Resource, Default)]
pub(crate) struct SpeciesAssets {
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
pub(crate) struct SpeciesView<'a> {
    handles: &'a [Handle<SpeciesAsset>],
    by_species: &'a BTreeMap<AssetId, Handle<SpeciesAsset>>,
    by_variant: &'a BTreeMap<AssetId, VariantHandle>,
    assets: &'a Assets<SpeciesAsset>,
}

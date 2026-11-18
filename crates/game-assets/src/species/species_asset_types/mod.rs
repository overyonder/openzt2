use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::{
    AssetId,
    species::{LifeStage, Sex, SpeciesDocument},
};

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
        self.discovered = true;
        self.handles = vec![handle.clone()];
        self.loaded_handles = vec![handle.clone()];
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
        self.rebuild_variant_indexes([(&handle, document, &[][..])]);
    }
}

#[derive(Resource, Default)]
pub struct SpeciesAssets {
    pub(super) discovered: bool,
    pub(super) dirty: bool,
    pub(super) pending_paths: std::collections::VecDeque<std::path::PathBuf>,
    pub(super) handles: Vec<Handle<SpeciesAsset>>,
    pub(super) loaded_handles: Vec<Handle<SpeciesAsset>>,
    pub(super) by_species: BTreeMap<AssetId, Handle<SpeciesAsset>>,
    pub(super) by_variant: BTreeMap<AssetId, VariantHandle>,
    variants_by_species: BTreeMap<AssetId, Vec<AssetId>>,
    variant_by_model_path: BTreeMap<String, VariantHandle>,
    animation_set_owners: BTreeMap<AssetId, Handle<SpeciesAsset>>,
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
    variants_by_species: &'a BTreeMap<AssetId, Vec<AssetId>>,
    variant_by_model_path: &'a BTreeMap<String, VariantHandle>,
    animation_set_owners: &'a BTreeMap<AssetId, Handle<SpeciesAsset>>,
    assets: &'a Assets<SpeciesAsset>,
}

impl SpeciesAssets {
    pub(super) fn rebuild_variant_indexes<'a>(
        &mut self,
        documents: impl IntoIterator<
            Item = (
                &'a Handle<SpeciesAsset>,
                &'a SpeciesDocument,
                &'a [SpeciesAnimationSetDependencyPath],
            ),
        >,
    ) {
        self.variant_by_model_path.clear();
        self.animation_set_owners.clear();
        self.variants_by_species.clear();
        let mut adult_fallbacks = BTreeMap::<AssetId, Vec<AssetId>>::new();
        let mut species_with_concrete_adult = std::collections::BTreeSet::new();
        for (handle, document, animation_sets) in documents {
            let mut variants_by_model_in_document = BTreeMap::new();
            for (index, binding) in document.variants.iter().enumerate() {
                let location = VariantHandle {
                    species: binding.species,
                    document: handle.clone(),
                    index,
                };
                variants_by_model_in_document
                    .entry(normalized_species_model_path(&binding.variant.model))
                    .or_insert_with(|| location.clone());
                let id = binding.variant.id;
                if !self.by_variant.get(&id).is_some_and(|winner| {
                    winner.document.id() == handle.id() && winner.index == index
                }) {
                    continue;
                }
                if binding.variant.life_stage == LifeStage::Adult {
                    if binding.variant.sex == Sex::Any {
                        adult_fallbacks.entry(binding.species).or_default().push(id);
                        continue;
                    }
                    species_with_concrete_adult.insert(binding.species);
                }
                self.variants_by_species
                    .entry(binding.species)
                    .or_default()
                    .push(id);
            }
            self.variant_by_model_path
                .extend(variants_by_model_in_document);
            self.animation_set_owners.extend(
                animation_sets
                    .iter()
                    .map(|dependency| (dependency.id, handle.clone())),
            );
        }
        for (species, fallbacks) in adult_fallbacks {
            if !species_with_concrete_adult.contains(&species) {
                self.variants_by_species
                    .entry(species)
                    .or_default()
                    .extend(fallbacks);
            }
        }
        for variants in self.variants_by_species.values_mut() {
            variants.sort_unstable();
        }
    }
}

fn normalized_species_model_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

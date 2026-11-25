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
    pub(super) variant_need_adjustments: Box<[(AssetId, i32)]>,
    pub(super) animation_sets: Box<[SpeciesAnimationSetDependencyPath]>,
}

#[cfg(any(test, feature = "testing"))]
impl SpeciesAsset {
    pub fn from_test_document(document: SpeciesDocument) -> Self {
        Self {
            document,
            variant_need_adjustments: Box::default(),
            animation_sets: Box::new([]),
        }
    }
}

#[derive(Resource, Default)]
pub struct SpeciesAssets {
    pub(super) discovered: bool,
    pub(super) binder_sources: Option<std::sync::Arc<crate::world_definitions::world_definition_asset_loading::authored_binder_reference_resolution::AuthoredBinderSources>>,
    pub(super) source_generation: u64,
    pub(super) source_set_changed: bool,
    pub(super) pending_paths: std::collections::VecDeque<std::path::PathBuf>,
    pub(super) pending: std::collections::BTreeSet<bevy::asset::AssetId<SpeciesAsset>>,
    pub(super) failed: std::collections::BTreeSet<bevy::asset::AssetId<SpeciesAsset>>,
    pub(super) handles: Vec<Handle<SpeciesAsset>>,
    pub(super) source_ids: std::collections::BTreeSet<bevy::asset::AssetId<SpeciesAsset>>,
    loaded_handles: Vec<Handle<SpeciesAsset>>,
    by_species: BTreeMap<AssetId, SpeciesDocumentRow>,
    by_variant: BTreeMap<AssetId, VariantLocation>,
    variants_by_species: BTreeMap<AssetId, Box<[VariantLocation]>>,
    variant_by_model_path: BTreeMap<String, VariantLocation>,
    animation_set_owners: BTreeMap<AssetId, SpeciesDocumentRow>,
}

#[derive(Clone, Copy)]
struct SpeciesDocumentRow {
    document: usize,
    index: usize,
}

#[derive(Clone, Copy)]
struct VariantLocation {
    row: SpeciesDocumentRow,
    animation_set: Option<SpeciesDocumentRow>,
}

#[derive(Clone, Copy)]
pub struct SpeciesView<'a> {
    index: &'a SpeciesAssets,
    assets: &'a Assets<SpeciesAsset>,
}

impl SpeciesAssets {
    #[cfg(any(test, feature = "testing"))]
    pub fn index_test_document(
        &mut self,
        handle: Handle<SpeciesAsset>,
        document: &SpeciesDocument,
    ) {
        self.discovered = true;
        self.handles = vec![handle.clone()];
        self.source_ids = std::collections::BTreeSet::from([handle.id()]);
        self.index_loaded_documents(&[(handle, document, &[])]);
    }

    pub(super) fn index_loaded_documents(
        &mut self,
        documents: &[(
            Handle<SpeciesAsset>,
            &SpeciesDocument,
            &[SpeciesAnimationSetDependencyPath],
        )],
    ) {
        self.loaded_handles = documents
            .iter()
            .map(|(handle, _, _)| handle.clone())
            .collect();
        self.by_species.clear();
        self.by_variant.clear();
        self.variant_by_model_path.clear();
        self.animation_set_owners.clear();
        for (document, (_, _, animation_sets)) in documents.iter().enumerate() {
            self.animation_set_owners
                .extend(
                    animation_sets
                        .iter()
                        .enumerate()
                        .map(|(index, dependency)| {
                            (dependency.id, SpeciesDocumentRow { document, index })
                        }),
                );
        }
        for (document, (_, species, _)) in documents.iter().enumerate() {
            self.by_species.extend(
                species
                    .species
                    .iter()
                    .enumerate()
                    .map(|(index, species)| (species.id, SpeciesDocumentRow { document, index })),
            );
            let mut models = BTreeMap::new();
            for (index, binding) in species.variants.iter().enumerate() {
                let location = VariantLocation {
                    row: SpeciesDocumentRow { document, index },
                    animation_set: self
                        .animation_set_owners
                        .get(&binding.variant.model_animation_set)
                        .copied(),
                };
                self.by_variant.insert(binding.variant.id, location);
                models
                    .entry(normalized_species_model_path(&binding.variant.model))
                    .or_insert(location);
            }
            self.variant_by_model_path.extend(models);
        }
        let mut variants_by_species = BTreeMap::<AssetId, Vec<VariantLocation>>::new();
        let mut species_with_concrete_adults = std::collections::BTreeSet::new();
        for location in self.by_variant.values() {
            let binding = &documents[location.row.document].1.variants[location.row.index];
            let variant = &binding.variant;
            if variant.probability == 0 {
                continue;
            }
            if variant.life_stage == LifeStage::Adult && variant.sex != Sex::Any {
                species_with_concrete_adults.insert(binding.species);
            }
            variants_by_species
                .entry(binding.species)
                .or_default()
                .push(*location);
        }
        self.variants_by_species = variants_by_species
            .into_iter()
            .map(|(species, mut variants)| {
                if species_with_concrete_adults.contains(&species) {
                    variants.retain(|location| {
                        let variant = &documents[location.row.document].1.variants
                            [location.row.index]
                            .variant;
                        variant.life_stage != LifeStage::Adult || variant.sex != Sex::Any
                    });
                }
                (species, variants.into_boxed_slice())
            })
            .collect();
    }
}

fn normalized_species_model_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

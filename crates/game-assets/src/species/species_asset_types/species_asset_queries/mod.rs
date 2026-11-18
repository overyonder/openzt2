use super::{SpeciesAsset, SpeciesAssets, SpeciesView, normalized_species_model_path};
use bevy::prelude::*;
use openzt2_game_data::{
    AssetId,
    species::{Species, SpeciesVariant, SpeciesVariantBinding},
};

impl SpeciesAssets {
    /// Pending catalogue batches must finish before a missing saved species is an error.
    pub fn catalogue_load_is_pending(&self, _server: &AssetServer) -> bool {
        !self.discovered || self.dirty
    }

    pub fn get<'a>(&'a self, assets: &'a Assets<SpeciesAsset>) -> Option<SpeciesView<'a>> {
        (!self.loaded_handles.is_empty()).then_some(SpeciesView {
            handles: &self.loaded_handles,
            by_species: &self.by_species,
            by_variant: &self.by_variant,
            variants_by_species: &self.variants_by_species,
            variant_by_model_path: &self.variant_by_model_path,
            animation_set_owners: &self.animation_set_owners,
            assets,
        })
    }
}

impl<'a> SpeciesView<'a> {
    pub(super) fn documents(self) -> impl Iterator<Item = &'a SpeciesAsset> {
        self.handles
            .iter()
            .rev()
            .filter_map(|handle| self.assets.get(handle))
    }

    pub fn find(self, id: AssetId) -> Option<&'a Species> {
        self.assets
            .get(self.by_species.get(&id)?)?
            .document
            .species
            .iter()
            .find(|species| species.id == id)
    }

    pub fn species(self) -> impl Iterator<Item = &'a Species> {
        self.documents()
            .flat_map(|asset| asset.document.species.iter())
    }

    pub fn variants(self, species: AssetId) -> impl Iterator<Item = &'a SpeciesVariant> {
        self.variants_by_species
            .get(&species)
            .into_iter()
            .flatten()
            .filter_map(move |id| self.find_variant(*id))
    }

    pub fn find_variant(self, id: AssetId) -> Option<&'a SpeciesVariant> {
        Some(&self.find_variant_binding(id)?.variant)
    }

    pub fn find_variant_binding(self, id: AssetId) -> Option<&'a SpeciesVariantBinding> {
        let declaration = self.by_variant.get(&id)?;
        let binding = self
            .assets
            .get(&declaration.document)?
            .document
            .variants
            .get(declaration.index)?;
        if binding.species != declaration.species || binding.variant.id != id {
            return None;
        }
        Some(binding)
    }

    pub fn find_variant_with_species(
        self,
        id: AssetId,
    ) -> Option<(&'a Species, &'a SpeciesVariant)> {
        let binding = self.find_variant_binding(id)?;
        Some((self.find(binding.species)?, &binding.variant))
    }

    /// Archive paths are case-insensitive: actor manifests and the resolved
    /// archive entry can spell the same model with different casing, so the
    /// paths compare by text rather than by their hashed identities.
    pub fn find_variant_by_model_path(self, model_path: &str) -> Option<&'a SpeciesVariant> {
        let model_path = normalized_species_model_path(model_path);
        let location = self.variant_by_model_path.get(&model_path)?;
        let variant = &self
            .assets
            .get(&location.document)?
            .document
            .variants
            .get(location.index)?
            .variant;
        (normalized_species_model_path(&variant.model) == model_path).then_some(variant)
    }
}

use super::{SpeciesAsset, SpeciesAssets, SpeciesView, VariantHandle};
use bevy::prelude::*;
use openzt2_game_data::{
    species::{Species, SpeciesVariant, SpeciesVariantBinding},
    AssetId,
};

impl SpeciesAssets {
    pub fn get<'a>(&'a self, assets: &'a Assets<SpeciesAsset>) -> Option<SpeciesView<'a>> {
        self.complete.then_some(SpeciesView {
            handles: &self.handles,
            by_species: &self.by_species,
            by_variant: &self.by_variant,
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

    fn variant(self, declaration: &VariantHandle) -> Option<&'a SpeciesVariant> {
        self.assets
            .get(&declaration.document)?
            .document
            .variants
            .get(declaration.index)
            .map(|binding| &binding.variant)
    }

    pub fn variants(self, species: AssetId) -> impl Iterator<Item = &'a SpeciesVariant> {
        let has_concrete_adult = self.by_variant.values().any(|declaration| {
            declaration.species == species
                && self.variant(declaration).is_some_and(|variant| {
                    variant.life_stage == openzt2_game_data::species::LifeStage::Adult
                        && variant.sex != openzt2_game_data::species::Sex::Any
                })
        });
        self.by_variant
            .values()
            .filter(move |declaration| declaration.species == species)
            .filter_map(move |declaration| {
                let variant = self.variant(declaration)?;
                (!(has_concrete_adult
                    && variant.life_stage == openzt2_game_data::species::LifeStage::Adult
                    && variant.sex == openzt2_game_data::species::Sex::Any))
                    .then_some(variant)
            })
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

    pub fn find_variant_by_model(self, model: AssetId) -> Option<&'a SpeciesVariant> {
        self.documents()
            .flat_map(|asset| &asset.document.variants)
            .map(|binding| &binding.variant)
            .find(|variant| AssetId::from_virtual_path(&variant.model) == model)
    }
}

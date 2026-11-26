use super::{
    normalized_species_model_path, SpeciesAsset, SpeciesAssets, SpeciesDocumentRow, SpeciesView,
};
use bevy::prelude::*;
use openzt2_game_data::{
    species::{NeedKind, Species, SpeciesVariant, SpeciesVariantBinding},
    AssetId,
};

impl SpeciesAssets {
    pub fn catalogue_load_is_pending(&self, _server: &AssetServer) -> bool {
        !self.discovered || self.source_set_changed
    }

    pub fn get<'a>(&'a self, assets: &'a Assets<SpeciesAsset>) -> Option<SpeciesView<'a>> {
        (!self.source_set_changed && !self.loaded_handles.is_empty()).then_some(SpeciesView {
            index: self,
            assets,
        })
    }
}

impl<'a> SpeciesView<'a> {
    fn document(self, index: usize) -> Option<&'a SpeciesAsset> {
        self.assets.get(self.index.loaded_handles.get(index)?)
    }

    fn variant_at(self, row: SpeciesDocumentRow) -> Option<&'a SpeciesVariantBinding> {
        self.document(row.document)?
            .document
            .variants
            .get(row.index)
    }

    pub fn find(self, id: AssetId) -> Option<&'a Species> {
        let row = self.index.by_species.get(&id)?;
        let species = self
            .document(row.document)?
            .document
            .species
            .get(row.index)?;
        (species.id == id).then_some(species)
    }

    pub fn species(self) -> impl Iterator<Item = &'a Species> {
        self.index
            .loaded_handles
            .iter()
            .rev()
            .filter_map(|handle| self.assets.get(handle))
            .flat_map(|asset| asset.document.species.iter())
    }

    pub fn variants(self, species: AssetId) -> impl Iterator<Item = &'a SpeciesVariant> {
        self.index
            .variants_by_species
            .get(&species)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter_map(move |location| {
                let binding = self.variant_at(location.row)?;
                (binding.species == species).then_some(&binding.variant)
            })
    }

    pub fn find_variant(self, id: AssetId) -> Option<&'a SpeciesVariant> {
        Some(&self.find_variant_binding(id)?.variant)
    }

    pub fn find_variant_binding(self, id: AssetId) -> Option<&'a SpeciesVariantBinding> {
        let binding = self.variant_at(self.index.by_variant.get(&id)?.row)?;
        (binding.variant.id == id).then_some(binding)
    }

    pub fn find_variant_with_species(
        self,
        id: AssetId,
    ) -> Option<(&'a Species, &'a SpeciesVariant)> {
        let binding = self.find_variant_binding(id)?;
        Some((self.find(binding.species)?, &binding.variant))
    }

    pub fn variant_need_adjustment(self, variant: AssetId, kind: NeedKind) -> Option<i32> {
        self.variant_need_adjustment_by_name(
            variant,
            crate::species::source::needs::authored_need_identifier(kind),
        )
    }

    /// Reads a binder-specific adjustment in wellness permille Q16 per update.
    pub fn variant_need_adjustment_by_name(self, variant: AssetId, need: AssetId) -> Option<i32> {
        let row = self.index.by_variant.get(&variant)?.row;
        let adjustments = &self.document(row.document)?.variant_need_adjustments;
        let index = adjustments
            .binary_search_by_key(&need, |(id, _)| *id)
            .ok()?;
        Some(adjustments[index].1)
    }

    pub fn find_variant_by_model_path(self, model_path: &str) -> Option<&'a SpeciesVariant> {
        let key = normalized_species_model_path(model_path);
        let variant = &self
            .variant_at(self.index.variant_by_model_path.get(&key)?.row)?
            .variant;
        (normalized_species_model_path(&variant.model) == key).then_some(variant)
    }
}

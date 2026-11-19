use super::SpeciesView;
use crate::animation::animation_set_asset_types::AnimationSetAsset;
use bevy::prelude::*;
use openzt2_game_data::species::SpeciesVariant;

impl SpeciesView<'_> {
    pub fn load_variant_animation_set(
        self,
        server: &AssetServer,
        variant: &SpeciesVariant,
    ) -> Option<Handle<AnimationSetAsset>> {
        let id = variant.model_animation_set;
        let path_at = |location: super::SpeciesDocumentRow| {
            let asset = self
                .assets
                .get(self.index.loaded_handles.get(location.document)?)?;
            let dependency = asset.animation_sets.get(location.index)?;
            (dependency.id == id).then_some(&*dependency.path)
        };
        let path = self
            .index
            .by_variant
            .get(&variant.id)
            .and_then(|location| location.animation_set)
            .and_then(path_at)
            .or_else(|| path_at(*self.index.animation_set_owners.get(&id)?))?;
        Some(server.load(path.to_owned()))
    }
}

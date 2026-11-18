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
        let asset = self.assets.get(self.animation_set_owners.get(&id)?)?;
        let index = asset
            .animation_sets
            .binary_search_by_key(&id, |dependency| dependency.id)
            .ok()?;
        Some(server.load(asset.animation_sets[index].path.to_string()))
    }
}

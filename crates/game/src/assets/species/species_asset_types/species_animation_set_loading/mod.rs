use super::SpeciesView;
use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;
use bevy::prelude::*;
use openzt2_game_data::species::SpeciesVariant;

impl SpeciesView<'_> {
    pub(crate) fn load_variant_animation_set(
        self,
        server: &AssetServer,
        variant: &SpeciesVariant,
    ) -> Option<Handle<AnimationSetAsset>> {
        self.documents()
            .find_map(|asset| {
                asset
                    .animation_sets
                    .binary_search_by_key(&variant.model_animation_set, |candidate| candidate.id)
                    .ok()
                    .map(|index| asset.animation_sets[index].path.to_string())
            })
            .map(|path| server.load(path))
    }
}
